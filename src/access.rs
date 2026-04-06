use std::path::Path;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub struct Caller {
    pub pid: Option<u32>,
    process_name: Arc<OnceLock<Option<String>>>,
}

impl Caller {
    pub fn new(pid: Option<u32>, process_name: Option<String>) -> Self {
        let process_name_cell = OnceLock::new();
        if process_name.is_some() || pid.is_none() {
            let _ = process_name_cell.set(process_name);
        }
        Self {
            pid,
            process_name: Arc::new(process_name_cell),
        }
    }

    pub fn process_name(&self) -> Option<&str> {
        self.process_name.get().and_then(|name| name.as_deref())
    }

    pub fn process_name_or_init<F>(&self, init: F) -> Option<&str>
    where
        F: FnOnce() -> Option<String>,
    {
        self.process_name.get_or_init(init).as_deref()
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
