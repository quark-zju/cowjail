use std::path::Path;
use std::sync::{Arc, OnceLock};

pub type ProcessNameGetter = fn(u32) -> Option<String>;

#[derive(Debug)]
pub struct Caller {
    pub pid: Option<u32>,
    process_name: OnceLock<Option<String>>,
    get_name: ProcessNameGetter,
}

impl Caller {
    pub fn new(pid: Option<u32>, get_name: ProcessNameGetter) -> Self {
        Self {
            pid,
            process_name: OnceLock::new(),
            get_name,
        }
    }

    pub fn with_process_name(pid: Option<u32>, process_name: Option<String>) -> Self {
        let process_name_cell = OnceLock::new();
        let _ = process_name_cell.set(process_name);
        Self {
            pid,
            process_name: process_name_cell,
            get_name: |_| None,
        }
    }

    pub fn process_name(&self) -> Option<&str> {
        let pid = self.pid?;
        self.process_name
            .get_or_init(|| (self.get_name)(pid))
            .as_deref()
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

    fn should_cache_readdir(&self, _path: &Path) -> bool {
        true
    }
}

impl<T: AccessController + ?Sized> AccessController for Arc<T> {
    fn check(&self, request: &AccessRequest<'_>) -> AccessDecision {
        (**self).check(request)
    }

    fn should_cache_readdir(&self, path: &Path) -> bool {
        (**self).should_cache_readdir(path)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAll;

impl AccessController for AllowAll {
    fn check(&self, _request: &AccessRequest<'_>) -> AccessDecision {
        AccessDecision::Allow
    }
}
