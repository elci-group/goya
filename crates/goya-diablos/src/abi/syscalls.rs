//! BSD syscall mappings per variant.

use std::collections::HashMap;

/// Syscall mapping for a BSD variant.
#[derive(Debug, Clone)]
pub struct SyscallMapping {
    pub variant: String,
    pub syscalls: HashMap<String, u32>,
}

impl SyscallMapping {
    pub fn new(variant: &str) -> Self {
        Self {
            variant: variant.to_string(),
            syscalls: HashMap::new(),
        }
    }

    pub fn add(&mut self, name: &str, number: u32) {
        self.syscalls.insert(name.to_string(), number);
    }

    pub fn get(&self, name: &str) -> Option<u32> {
        self.syscalls.get(name).copied()
    }
}

/// FreeBSD syscall numbers (x86_64).
pub fn freebsd_syscalls() -> SyscallMapping {
    let mut m = SyscallMapping::new("freebsd");
    // Common syscalls
    m.add("read", 3);
    m.add("write", 4);
    m.add("open", 5);
    m.add("close", 6);
    m.add("stat", 188);
    m.add("fstat", 189);
    m.add("lstat", 190);
    m.add("poll", 209);
    m.add("mmap", 477);
    m.add("munmap", 478);
    m.add("brk", 45);
    m.add("rt_sigaction", 174);
    m.add("rt_sigprocmask", 175);
    m.add("rt_sigreturn", 173);
    m.add("ioctl", 54);
    m.add("pread64", 67);
    m.add("pwrite64", 68);
    m.add("readv", 120);
    m.add("writev", 121);
    m.add("access", 33);
    m.add("pipe", 42);
    m.add("select", 93);
    m.add("sched_yield", 158);
    m.add("mremap", 479);
    m.add("msync", 480);
    m.add("mincore", 481);
    m.add("madvise", 482);
    m.add("shmget", 241);
    m.add("shmat", 242);
    m.add("shmctl", 243);
    m.add("dup", 41);
    m.add("dup2", 90);
    m.add("pause", 29);
    m.add("nanosleep", 240);
    m.add("getitimer", 105);
    m.add("alarm", 27);
    m.add("setitimer", 104);
    m.add("getpid", 20);
    m.add("sendfile", 393);
    m.add("socket", 97);
    m.add("connect", 98);
    m.add("accept", 99);
    m.add("sendto", 100);
    m.add("recvfrom", 101);
    m.add("sendmsg", 102);
    m.add("recvmsg", 103);
    m.add("shutdown", 104);
    m.add("bind", 105);
    m.add("listen", 106);
    m.add("getsockname", 107);
    m.add("getpeername", 108);
    m.add("socketpair", 109);
    m.add("setsockopt", 110);
    m.add("getsockopt", 111);
    m.add("clone", 120);
    m.add("fork", 2);
    m.add("vfork", 190);
    m.add("execve", 59);
    m.add("exit", 1);
    m.add("wait4", 7);
    m.add("kill", 37);
    m.add("uname", 122);
    m.add("semget", 221);
    m.add("semop", 222);
    m.add("semctl", 223);
    m.add("shmdt", 244);
    m.add("msgget", 224);
    m.add("msgsnd", 225);
    m.add("msgrcv", 226);
    m.add("msgctl", 227);
    m.add("fcntl", 92);
    m.add("flock", 143);
    m.add("fsync", 118);
    m.add("fdatasync", 484);
    m.add("truncate", 192);
    m.add("ftruncate", 193);
    m.add("getdents", 272);
    m.add("getcwd", 183);
    m.add("chdir", 12);
    m.add("fchdir", 133);
    m.add("rename", 128);
    m.add("mkdir", 136);
    m.add("rmdir", 137);
    m.add("creat", 85);
    m.add("link", 127);
    m.add("unlink", 10);
    m.add("symlink", 188);
    m.add("readlink", 189);
    m.add("chmod", 15);
    m.add("fchmod", 195);
    m.add("chown", 182);
    m.add("fchown", 194);
    m.add("lchown", 196);
    m.add("umask", 166);
    m.add("gettimeofday", 169);
    m.add("getrlimit", 191);
    m.add("getrusage", 98);
    m.add("sysinfo", 116);
    m.add("times", 101);
    m.add("ptrace", 26);
    m.add("getuid", 24);
    m.add("syslog", 103);
    m.add("getgid", 47);
    m.add("setuid", 23);
    m.add("setgid", 46);
    m.add("geteuid", 25);
    m.add("getegid", 48);
    m.add("setpgid", 114);
    m.add("getppid", 39);
    m.add("getpgrp", 65);
    m.add("setsid", 112);
    m.add("setreuid", 113);
    m.add("setregid", 114);
    m.add("getgroups", 80);
    m.add("setgroups", 81);
    m.add("setresuid", 115);
    m.add("getresuid", 116);
    m.add("setresgid", 117);
    m.add("getresgid", 118);
    m.add("getpgid", 121);
    m.add("setfsuid", 120);
    m.add("setfsgid", 121);
    m.add("getsid", 124);
    m.add("cap_enter", 540);
    m.add("cap_getmode", 541);
    m.add("cap_rights_limit", 542);
    m.add("cap_ioctls_limit", 543);
    m.add("cap_fcntls_limit", 544);
    m.add("cap_new", 545);
    m.add("cap_get", 546);
    m.add("cap_set", 547);
    m.add("cap_wrap", 548);
    m.add("cap_unwrap", 549);
    m.add("jail", 207);
    m.add("jail_attach", 208);
    m.add("jail_remove", 209);
    m.add("jail_get", 210);
    m.add("jail_set", 211);
    m.add("kqueue", 362);
    m.add("kevent", 363);
    m
}

/// OpenBSD syscall numbers (x86_64).
pub fn openbsd_syscalls() -> SyscallMapping {
    let mut m = SyscallMapping::new("openbsd");
    m.add("read", 3);
    m.add("write", 4);
    m.add("open", 5);
    m.add("close", 6);
    m.add("stat", 18);
    m.add("fstat", 28);
    m.add("lstat", 29);
    m.add("poll", 209);
    m.add("mmap", 197);
    m.add("munmap", 198);
    m.add("brk", 45);
    m.add("sigaction", 174);
    m.add("sigprocmask", 175);
    m.add("ioctl", 54);
    m.add("pread", 67);
    m.add("pwrite", 68);
    m.add("readv", 120);
    m.add("writev", 121);
    m.add("access", 33);
    m.add("pipe", 42);
    m.add("select", 93);
    m.add("sched_yield", 158);
    m.add("mremap", 199);
    m.add("msync", 200);
    m.add("mincore", 201);
    m.add("madvise", 202);
    m.add("shmget", 241);
    m.add("shmat", 242);
    m.add("shmctl", 243);
    m.add("dup", 41);
    m.add("dup2", 90);
    m.add("pause", 29);
    m.add("nanosleep", 240);
    m.add("getitimer", 105);
    m.add("alarm", 27);
    m.add("setitimer", 104);
    m.add("getpid", 20);
    m.add("sendfile", 393);
    m.add("socket", 97);
    m.add("connect", 98);
    m.add("accept", 99);
    m.add("sendto", 100);
    m.add("recvfrom", 101);
    m.add("sendmsg", 102);
    m.add("recvmsg", 103);
    m.add("shutdown", 104);
    m.add("bind", 105);
    m.add("listen", 106);
    m.add("getsockname", 107);
    m.add("getpeername", 108);
    m.add("socketpair", 109);
    m.add("setsockopt", 110);
    m.add("getsockopt", 111);
    m.add("clone", 120);
    m.add("fork", 2);
    m.add("vfork", 190);
    m.add("execve", 59);
    m.add("exit", 1);
    m.add("wait4", 7);
    m.add("kill", 37);
    m.add("uname", 122);
    m.add("semget", 221);
    m.add("semop", 222);
    m.add("semctl", 223);
    m.add("shmdt", 244);
    m.add("msgget", 224);
    m.add("msgsnd", 225);
    m.add("msgrcv", 226);
    m.add("msgctl", 227);
    m.add("fcntl", 92);
    m.add("flock", 143);
    m.add("fsync", 118);
    m.add("fdatasync", 484);
    m.add("truncate", 192);
    m.add("ftruncate", 193);
    m.add("getdents", 272);
    m.add("getcwd", 183);
    m.add("chdir", 12);
    m.add("fchdir", 133);
    m.add("rename", 128);
    m.add("mkdir", 136);
    m.add("rmdir", 137);
    m.add("creat", 85);
    m.add("link", 127);
    m.add("unlink", 10);
    m.add("symlink", 188);
    m.add("readlink", 189);
    m.add("chmod", 15);
    m.add("fchmod", 195);
    m.add("chown", 182);
    m.add("fchown", 194);
    m.add("lchown", 196);
    m.add("umask", 166);
    m.add("gettimeofday", 169);
    m.add("getrlimit", 191);
    m.add("getrusage", 98);
    m.add("sysinfo", 116);
    m.add("times", 101);
    m.add("ptrace", 26);
    m.add("getuid", 24);
    m.add("syslog", 103);
    m.add("getgid", 47);
    m.add("setuid", 23);
    m.add("setgid", 46);
    m.add("geteuid", 25);
    m.add("getegid", 48);
    m.add("setpgid", 114);
    m.add("getppid", 39);
    m.add("getpgrp", 65);
    m.add("setsid", 112);
    m.add("setreuid", 113);
    m.add("setregid", 114);
    m.add("getgroups", 80);
    m.add("setgroups", 81);
    m.add("setresuid", 115);
    m.add("getresuid", 116);
    m.add("setresgid", 117);
    m.add("getresgid", 118);
    m.add("getpgid", 121);
    m.add("setfsuid", 120);
    m.add("setfsgid", 121);
    m.add("getsid", 124);
    m.add("pledge", 443);
    m.add("unveil", 444);
    m.add("kqueue", 362);
    m.add("kevent", 363);
    m
}

/// NetBSD syscall numbers (x86_64).
pub fn netbsd_syscalls() -> SyscallMapping {
    let mut m = SyscallMapping::new("netbsd");
    m.add("read", 3);
    m.add("write", 4);
    m.add("open", 5);
    m.add("close", 6);
    m.add("stat", 188);
    m.add("fstat", 189);
    m.add("lstat", 190);
    m.add("poll", 209);
    m.add("mmap", 477);
    m.add("munmap", 478);
    m.add("brk", 45);
    m.add("sigaction", 174);
    m.add("sigprocmask", 175);
    m.add("ioctl", 54);
    m.add("pread", 67);
    m.add("pwrite", 68);
    m.add("readv", 120);
    m.add("writev", 121);
    m.add("access", 33);
    m.add("pipe", 42);
    m.add("select", 93);
    m.add("sched_yield", 158);
    m.add("mremap", 479);
    m.add("msync", 480);
    m.add("mincore", 481);
    m.add("madvise", 482);
    m.add("shmget", 241);
    m.add("shmat", 242);
    m.add("shmctl", 243);
    m.add("dup", 41);
    m.add("dup2", 90);
    m.add("pause", 29);
    m.add("nanosleep", 240);
    m.add("getitimer", 105);
    m.add("alarm", 27);
    m.add("setitimer", 104);
    m.add("getpid", 20);
    m.add("sendfile", 393);
    m.add("socket", 97);
    m.add("connect", 98);
    m.add("accept", 99);
    m.add("sendto", 100);
    m.add("recvfrom", 101);
    m.add("sendmsg", 102);
    m.add("recvmsg", 103);
    m.add("shutdown", 104);
    m.add("bind", 105);
    m.add("listen", 106);
    m.add("getsockname", 107);
    m.add("getpeername", 108);
    m.add("socketpair", 109);
    m.add("setsockopt", 110);
    m.add("getsockopt", 111);
    m.add("clone", 120);
    m.add("fork", 2);
    m.add("vfork", 190);
    m.add("execve", 59);
    m.add("exit", 1);
    m.add("wait4", 7);
    m.add("kill", 37);
    m.add("uname", 122);
    m.add("semget", 221);
    m.add("semop", 222);
    m.add("semctl", 223);
    m.add("shmdt", 244);
    m.add("msgget", 224);
    m.add("msgsnd", 225);
    m.add("msgrcv", 226);
    m.add("msgctl", 227);
    m.add("fcntl", 92);
    m.add("flock", 143);
    m.add("fsync", 118);
    m.add("fdatasync", 484);
    m.add("truncate", 192);
    m.add("ftruncate", 193);
    m.add("getdents", 272);
    m.add("getcwd", 183);
    m.add("chdir", 12);
    m.add("fchdir", 133);
    m.add("rename", 128);
    m.add("mkdir", 136);
    m.add("rmdir", 137);
    m.add("creat", 85);
    m.add("link", 127);
    m.add("unlink", 10);
    m.add("symlink", 188);
    m.add("readlink", 189);
    m.add("chmod", 15);
    m.add("fchmod", 195);
    m.add("chown", 182);
    m.add("fchown", 194);
    m.add("lchown", 196);
    m.add("umask", 166);
    m.add("gettimeofday", 169);
    m.add("getrlimit", 191);
    m.add("getrusage", 98);
    m.add("sysinfo", 116);
    m.add("times", 101);
    m.add("ptrace", 26);
    m.add("getuid", 24);
    m.add("syslog", 103);
    m.add("getgid", 47);
    m.add("setuid", 23);
    m.add("setgid", 46);
    m.add("geteuid", 25);
    m.add("getegid", 48);
    m.add("setpgid", 114);
    m.add("getppid", 39);
    m.add("getpgrp", 65);
    m.add("setsid", 112);
    m.add("setreuid", 113);
    m.add("setregid", 114);
    m.add("getgroups", 80);
    m.add("setgroups", 81);
    m.add("setresuid", 115);
    m.add("getresuid", 116);
    m.add("setresgid", 117);
    m.add("getresgid", 118);
    m.add("getpgid", 121);
    m.add("setfsuid", 120);
    m.add("setfsgid", 121);
    m.add("getsid", 124);
    m.add("kauth_authorize", 500);
    m.add("kqueue", 362);
    m.add("kevent", 363);
    m
}

/// DragonFly BSD syscall numbers (x86_64).
pub fn dragonfly_syscalls() -> SyscallMapping {
    let mut m = SyscallMapping::new("dragonfly");
    m.add("read", 3);
    m.add("write", 4);
    m.add("open", 5);
    m.add("close", 6);
    m.add("stat", 188);
    m.add("fstat", 189);
    m.add("lstat", 190);
    m.add("poll", 209);
    m.add("mmap", 477);
    m.add("munmap", 478);
    m.add("brk", 45);
    m.add("sigaction", 174);
    m.add("sigprocmask", 175);
    m.add("ioctl", 54);
    m.add("pread", 67);
    m.add("pwrite", 68);
    m.add("readv", 120);
    m.add("writev", 121);
    m.add("access", 33);
    m.add("pipe", 42);
    m.add("select", 93);
    m.add("sched_yield", 158);
    m.add("mremap", 479);
    m.add("msync", 480);
    m.add("mincore", 481);
    m.add("madvise", 482);
    m.add("shmget", 241);
    m.add("shmat", 242);
    m.add("shmctl", 243);
    m.add("dup", 41);
    m.add("dup2", 90);
    m.add("pause", 29);
    m.add("nanosleep", 240);
    m.add("getitimer", 105);
    m.add("alarm", 27);
    m.add("setitimer", 104);
    m.add("getpid", 20);
    m.add("sendfile", 393);
    m.add("socket", 97);
    m.add("connect", 98);
    m.add("accept", 99);
    m.add("sendto", 100);
    m.add("recvfrom", 101);
    m.add("sendmsg", 102);
    m.add("recvmsg", 103);
    m.add("shutdown", 104);
    m.add("bind", 105);
    m.add("listen", 106);
    m.add("getsockname", 107);
    m.add("getpeername", 108);
    m.add("socketpair", 109);
    m.add("setsockopt", 110);
    m.add("getsockopt", 111);
    m.add("clone", 120);
    m.add("fork", 2);
    m.add("vfork", 190);
    m.add("execve", 59);
    m.add("exit", 1);
    m.add("wait4", 7);
    m.add("kill", 37);
    m.add("uname", 122);
    m.add("semget", 221);
    m.add("semop", 222);
    m.add("semctl", 223);
    m.add("shmdt", 244);
    m.add("msgget", 224);
    m.add("msgsnd", 225);
    m.add("msgrcv", 226);
    m.add("msgctl", 227);
    m.add("fcntl", 92);
    m.add("flock", 143);
    m.add("fsync", 118);
    m.add("fdatasync", 484);
    m.add("truncate", 192);
    m.add("ftruncate", 193);
    m.add("getdents", 272);
    m.add("getcwd", 183);
    m.add("chdir", 12);
    m.add("fchdir", 133);
    m.add("rename", 128);
    m.add("mkdir", 136);
    m.add("rmdir", 137);
    m.add("creat", 85);
    m.add("link", 127);
    m.add("unlink", 10);
    m.add("symlink", 188);
    m.add("readlink", 189);
    m.add("chmod", 15);
    m.add("fchmod", 195);
    m.add("chown", 182);
    m.add("fchown", 194);
    m.add("lchown", 196);
    m.add("umask", 166);
    m.add("gettimeofday", 169);
    m.add("getrlimit", 191);
    m.add("getrusage", 98);
    m.add("sysinfo", 116);
    m.add("times", 101);
    m.add("ptrace", 26);
    m.add("getuid", 24);
    m.add("syslog", 103);
    m.add("getgid", 47);
    m.add("setuid", 23);
    m.add("setgid", 46);
    m.add("geteuid", 25);
    m.add("getegid", 48);
    m.add("setpgid", 114);
    m.add("getppid", 39);
    m.add("getpgrp", 65);
    m.add("setsid", 112);
    m.add("setreuid", 113);
    m.add("setregid", 114);
    m.add("getgroups", 80);
    m.add("setgroups", 81);
    m.add("setresuid", 115);
    m.add("getresuid", 116);
    m.add("setresgid", 117);
    m.add("getresgid", 118);
    m.add("getpgid", 121);
    m.add("setfsuid", 120);
    m.add("setfsgid", 121);
    m.add("getsid", 124);
    m.add("kqueue", 362);
    m.add("kevent", 363);
    m.add("lwp_create", 500);
    m.add("lwp_exit", 501);
    m.add("lwp_wait", 502);
    m.add("lwp_suspend", 503);
    m.add("lwp_continue", 504);
    m.add("lwp_setprivate", 505);
    m.add("lwp_getprivate", 506);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freebsd_syscalls_exist() {
        let m = freebsd_syscalls();
        assert_eq!(m.get("read"), Some(3));
        assert_eq!(m.get("write"), Some(4));
        assert_eq!(m.get("cap_enter"), Some(540));
        assert_eq!(m.get("jail"), Some(207));
        assert_eq!(m.get("kqueue"), Some(362));
    }

    #[test]
    fn openbsd_syscalls_exist() {
        let m = openbsd_syscalls();
        assert_eq!(m.get("read"), Some(3));
        assert_eq!(m.get("write"), Some(4));
        assert_eq!(m.get("pledge"), Some(443));
        assert_eq!(m.get("unveil"), Some(444));
        assert_eq!(m.get("kqueue"), Some(362));
    }

    #[test]
    fn netbsd_syscalls_exist() {
        let m = netbsd_syscalls();
        assert_eq!(m.get("read"), Some(3));
        assert_eq!(m.get("write"), Some(4));
        assert_eq!(m.get("kauth_authorize"), Some(500));
        assert_eq!(m.get("kqueue"), Some(362));
    }

    #[test]
    fn dragonfly_syscalls_exist() {
        let m = dragonfly_syscalls();
        assert_eq!(m.get("read"), Some(3));
        assert_eq!(m.get("write"), Some(4));
        assert_eq!(m.get("lwp_create"), Some(500));
        assert_eq!(m.get("kqueue"), Some(362));
    }
}