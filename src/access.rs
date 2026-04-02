use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub pid: Option<u32>,
    pub process_name: Option<String>,
}

impl Caller {
    pub fn new(pid: Option<u32>, process_name: Option<String>) -> Self {
        Self { pid, process_name }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Lookup,
    GetAttr,
    ReadDir,
    ReadLink,
    OpenRead,
    OpenWrite,
    Read,
    Write,
    Create,
    Mkdir,
    Unlink,
    Rmdir,
    Symlink,
    Link,
    Rename,
    SetAttr,
    Access,
    Fsync,
    FsyncDir,
    StatFs,
    GetLock,
    SetReadLock,
    SetWriteLock,
    Unlock,
}

impl Operation {
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Self::OpenWrite
                | Self::Write
                | Self::Create
                | Self::Mkdir
                | Self::Unlink
                | Self::Rmdir
                | Self::Symlink
                | Self::Link
                | Self::Rename
                | Self::SetAttr
                | Self::SetWriteLock
                | Self::Fsync
                | Self::FsyncDir
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    Allow,
    Deny(i32),
}

#[derive(Debug, Clone, Copy)]
pub struct AccessRequest<'a> {
    pub caller: &'a Caller,
    pub path: &'a Path,
    pub operation: Operation,
}

pub trait AccessController: Send + Sync + 'static {
    fn check(&self, request: &AccessRequest<'_>) -> AccessDecision;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAll;

impl AccessController for AllowAll {
    fn check(&self, _request: &AccessRequest<'_>) -> AccessDecision {
        AccessDecision::Allow
    }
}
