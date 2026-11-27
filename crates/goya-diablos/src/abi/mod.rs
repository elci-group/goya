//! BSD ABI resolution and ELF handling.

pub mod elf;
pub mod syscalls;

pub use elf::{BsdElf, ElfVerificationError, verify_elf};
pub use syscalls::{SyscallMapping, freebsd_syscalls, openbsd_syscalls, netbsd_syscalls, dragonfly_syscalls};