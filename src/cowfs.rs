use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use fuse::{self, FileAttr, FileType, Filesystem, ReplyAttr, ReplyDirectory, ReplyEntry, Request};
use libc::ENOENT;

use crate::profile::Profile;

const TTL: Duration = Duration::from_secs(1);
const ROOT_INO: u64 = 1;

pub struct CowFs {
    _profile: Profile,
    uid: u32,
    gid: u32,
}

impl CowFs {
    pub fn new(profile: Profile) -> Self {
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        Self {
            _profile: profile,
            uid,
            gid,
        }
    }

    pub fn mount(self, mountpoint: &Path) -> Result<()> {
        let options = [
            OsStr::new("-o"),
            OsStr::new("default_permissions"),
            OsStr::new("-o"),
            OsStr::new("fsname=cowjail"),
        ];
        fuse::mount(self, mountpoint, &options).with_context(|| {
            format!(
                "failed to mount fuse filesystem at {}",
                mountpoint.display()
            )
        })
    }

    fn root_attr(&self) -> FileAttr {
        let now = SystemTime::now();
        FileAttr {
            ino: ROOT_INO,
            size: 0,
            blocks: 0,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            flags: 0,
        }
    }
}

impl Filesystem for CowFs {
    fn lookup(&mut self, _req: &Request<'_>, _parent: u64, _name: &OsStr, reply: ReplyEntry) {
        reply.error(ENOENT);
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyAttr) {
        if ino == ROOT_INO {
            reply.attr(&TTL, &self.root_attr());
        } else {
            reply.error(ENOENT);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino != ROOT_INO {
            reply.error(ENOENT);
            return;
        }

        let entries = [
            (ROOT_INO, FileType::Directory, "."),
            (ROOT_INO, FileType::Directory, ".."),
        ];
        for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
            reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);
        }
        reply.ok();
    }
}
