//! Isolated Windows clipboard transport. The host owns authorization and editor revalidation.
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { Denied, InvalidOwner, TooLarge, InvalidText, Unavailable, NativeFailure, ClipboardCleared }

fn encode(text: &str) -> Result<Vec<u16>, Error> {
    if text.len() > MAX_BYTES { return Err(Error::TooLarge); }
    if text.contains('\0') { return Err(Error::InvalidText); }
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    Ok(units)
}

fn decode(units: &[u16]) -> Result<String, Error> {
    let end = units.iter().position(|&u| u == 0).ok_or(Error::InvalidText)?;
    let mut text = String::new();
    let mut chars = std::char::decode_utf16(units[..end].iter().copied()).peekable();
    while let Some(c) = chars.next() {
        let c = c.map_err(|_| Error::InvalidText)?;
        let c = if c == '\r' {
            if matches!(chars.peek(), Some(Ok('\n'))) { chars.next(); }
            '\n'
        } else { c };
        if text.len().checked_add(c.len_utf8()).ok_or(Error::TooLarge)? > MAX_BYTES { return Err(Error::TooLarge); }
        text.push(c);
    }
    Ok(text)
}

#[cfg(windows)]
mod native {
    use super::{decode, encode, Error, MAX_BYTES};
    use std::ffi::c_void;
    use std::ptr::null_mut;
    type Handle = *mut c_void;
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 2;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn SetClipboardData(format: u32, memory: Handle) -> Handle;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalFree(memory: Handle) -> Handle;
        fn GlobalLock(memory: Handle) -> *mut c_void;
        fn GlobalUnlock(memory: Handle) -> i32;
        fn GlobalSize(memory: Handle) -> usize;
    }
    struct Clipboard;
    impl Clipboard {
        fn open(owner: Handle) -> Result<Self, Error> {
            if owner.is_null() { return Err(Error::InvalidOwner); }
            if unsafe { OpenClipboard(owner) } == 0 { return Err(Error::Unavailable); }
            Ok(Self)
        }
    }
    impl Drop for Clipboard { fn drop(&mut self) { unsafe { CloseClipboard(); } } }
    struct Memory(Handle);
    impl Drop for Memory { fn drop(&mut self) { if !self.0.is_null() { unsafe { GlobalFree(self.0); } } } }
    struct Locked(Handle);
    impl Drop for Locked { fn drop(&mut self) { unsafe { GlobalUnlock(self.0); } } }

    /// Invoke only for a user paste; `admit` checks live authorization.
    /// Owner must be a valid HWND belonging to the calling UI thread.
    pub fn read(owner: Handle, admit: impl FnOnce() -> bool) -> Result<String, Error> {
        if !admit() { return Err(Error::Denied); }
        let _clipboard = Clipboard::open(owner)?;
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 { return Err(Error::Unavailable); }
        let memory = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if memory.is_null() { return Err(Error::NativeFailure); }
        let size = unsafe { GlobalSize(memory) };
        if size == 0 || size % 2 != 0 { return Err(Error::InvalidText); }
        if size > (MAX_BYTES + 1) * 2 { return Err(Error::TooLarge); }
        let ptr = unsafe { GlobalLock(memory) } as *const u16;
        if ptr.is_null() { return Err(Error::NativeFailure); }
        let _lock = Locked(memory);
        decode(unsafe { std::slice::from_raw_parts(ptr, size / 2) })
    }

    /// Invoke only for a user copy/cut. Success acknowledges OS ownership;
    /// a cut must still revalidate before deleting local text.
    pub fn write(owner: Handle, text: &str, admit: impl FnOnce() -> bool) -> Result<(), Error> {
        if !admit() { return Err(Error::Denied); }
        if owner.is_null() { return Err(Error::InvalidOwner); }
        let units = encode(text)?;
        let mut memory = Memory(unsafe { GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) });
        if memory.0.is_null() { return Err(Error::NativeFailure); }
        let ptr = unsafe { GlobalLock(memory.0) } as *mut u16;
        if ptr.is_null() { return Err(Error::NativeFailure); }
        unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len()); GlobalUnlock(memory.0); }
        let _clipboard = Clipboard::open(owner)?;
        if unsafe { EmptyClipboard() } == 0 { return Err(Error::NativeFailure); }
        if unsafe { SetClipboardData(CF_UNICODETEXT, memory.0) }.is_null() { return Err(Error::ClipboardCleared); }
        memory.0 = null_mut();
        Ok(())
    }
}
#[cfg(windows)]
pub use native::{read, write};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_and_newlines() {
        assert_eq!(decode(&encode("a\r\n😀\rb").unwrap()), Ok("a\n😀\nb".into()));
        assert_eq!(decode(&[0]), Ok(String::new()));
    }
    #[test]
    fn rejects_invalid_and_oversized() {
        assert_eq!(encode("a\0b"), Err(Error::InvalidText));
        assert_eq!(decode(&[0xd800, 0]), Err(Error::InvalidText));
        assert_eq!(decode(&[65]), Err(Error::InvalidText));
        assert_eq!(encode(&"x".repeat(MAX_BYTES + 1)), Err(Error::TooLarge));
        let mut large = vec![65; MAX_BYTES + 1]; large.push(0);
        assert_eq!(decode(&large), Err(Error::TooLarge));
    }
}
