# Windows clipboard transport

`clipboard.rs` is an isolated, experimental Win32 CF_UNICODETEXT transport. It
is not wired to an editor or an admitted Gestures route. A host must provide a
valid UI-thread HWND, invoke it only for an explicit user action, and enforce
live policy in the callback, denying unknown state. It must prevent alternative
unguarded native routes. There is no polling, payload logging or fallback.

Paste callers must reserve and revalidate target, focus, lifecycle, revision,
selection, policy and replacement constraints before committing returned text.
Cut callers must reserve and prevalidate, export first, then revalidate before
deleting. Failure after export is a partial export and retains local text.
`ClipboardCleared` means the OS clipboard was emptied but export failed; never
overwrite it as compensation. Other errors also leave local editor state alone.
An empty selection returns `EmptySelection` before allocation or clipboard
access and leaves the clipboard unchanged; empty text remains valid on paste.

Input is strict UTF-16 terminated within bounded global memory, converted to at
most 1 MiB UTF-8, with CRLF and CR normalized to LF. Exports reject NUL and
limit UTF-8 to 1 MiB. The limit is provisional. Windows may synthesize Unicode
text from another native text format; the adapter requests only CF_UNICODETEXT
and cannot establish the originating format. The API is synchronous; a read
can block while another application renders delayed clipboard data. Windows
runtime behavior, accessibility and complete route coverage remain unverified;
no native qualification is claimed.
