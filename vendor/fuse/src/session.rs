//! Filesystem session
//!
//! A session runs a filesystem implementation while it is being mounted to a specific mount
//! point. A session begins by mounting the filesystem and ends by unmounting it. While the
//! filesystem is mounted, the session loop receives, dispatches and replies to kernel requests
//! for filesystem operations under its mount point.

use std::io;
use std::ffi::OsStr;
use std::fmt;
use std::path::{PathBuf, Path};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use thread_scoped::{scoped, JoinGuard};
use libc::{EAGAIN, EINTR, ENODEV, ENOENT};
use log::{error, info};

use crate::channel::{self, Channel};
use crate::request::Request;
use crate::Filesystem;

/// The max size of write requests from the kernel. The absolute minimum is 4k,
/// FUSE recommends at least 128k, max 16M. The FUSE default is 16M on macOS
/// and 128k on other systems.
pub const MAX_WRITE_SIZE: usize = 16 * 1024 * 1024;

/// Size of the buffer for reading a request from the kernel. Since the kernel may send
/// up to MAX_WRITE_SIZE bytes in a write request, we use that value plus some extra space.
const BUFFER_SIZE: usize = MAX_WRITE_SIZE + 4096;

/// The session data structure
#[derive(Debug)]
pub struct Session<FS: Filesystem> {
    /// Filesystem operation implementations
    pub filesystem: FS,
    /// Communication channel to the kernel driver
    ch: Channel,
    /// FUSE protocol major version
    pub proto_major: u32,
    /// FUSE protocol minor version
    pub proto_minor: u32,
    /// True if the filesystem is initialized (init operation done)
    pub initialized: bool,
    /// True if the filesystem was destroyed (destroy operation done)
    pub destroyed: bool,
}

impl<FS: Filesystem> Session<FS> {
    /// Create a new session by mounting the given filesystem to the given mountpoint
    pub fn new(filesystem: FS, mountpoint: &Path, options: &[&OsStr]) -> io::Result<Session<FS>> {
        info!("Mounting {}", mountpoint.display());
        Channel::new(mountpoint, options).map(|ch| {
            Session {
                filesystem: filesystem,
                ch: ch,
                proto_major: 0,
                proto_minor: 0,
                initialized: false,
                destroyed: false,
            }
        })
    }

    /// Return path of the mounted filesystem
    pub fn mountpoint(&self) -> &Path {
        &self.ch.mountpoint()
    }

    /// Run the session loop that receives kernel requests and dispatches them to method
    /// calls into the filesystem. This read-dispatch-loop is non-concurrent to prevent
    /// having multiple buffers (which take up much memory), but the filesystem methods
    /// may run concurrent by spawning threads.
    pub fn run(&mut self) -> io::Result<()> {
        // Buffer for receiving requests from the kernel. Only one is allocated and
        // it is reused immediately after dispatching to conserve memory and allocations.
        let mut buffer: Vec<u8> = Vec::with_capacity(BUFFER_SIZE);
        loop {
            // Read the next request from the given channel to kernel driver
            // The kernel driver makes sure that we get exactly one request per read
            match self.ch.receive(&mut buffer) {
                Ok(()) => match Request::new(self.ch.sender(), &buffer) {
                    // Dispatch request
                    Some(req) => req.dispatch(self),
                    // Quit loop on illegal request
                    None => break,
                },
                Err(err) => match err.raw_os_error() {
                    // Operation interrupted. Accordingly to FUSE, this is safe to retry
                    Some(ENOENT) => continue,
                    // Interrupted system call, retry
                    Some(EINTR) => continue,
                    // Explicitly try again
                    Some(EAGAIN) => continue,
                    // Filesystem was unmounted, quit the loop
                    Some(ENODEV) => break,
                    // Unhandled error
                    _ => return Err(err),
                }
            }
        }
        Ok(())
    }
}

impl<'a, FS: Filesystem + Send + 'a> Session<FS> {
    /// Run the session loop in a background thread
    pub unsafe fn spawn(self) -> io::Result<BackgroundSession<'a>> {
        BackgroundSession::new(self)
    }
}

impl<FS: Filesystem> Drop for Session<FS> {
    fn drop(&mut self) {
        info!("Unmounted {}", self.mountpoint().display());
    }
}

/// The background session data structure
pub struct BackgroundSession<'a> {
    /// Path of the mounted filesystem
    pub mountpoint: PathBuf,
    /// Thread guard of the background session
    pub guard: JoinGuard<'a, io::Result<()>>,
    /// True while the session thread is running.
    pub alive: Arc<AtomicBool>,
    /// Last raw_os_error observed from the session loop.
    pub last_error_code: Arc<Mutex<Option<i32>>>,
}

impl<'a> BackgroundSession<'a> {
    /// Create a new background session for the given session by running its
    /// session loop in a background thread. If the returned handle is dropped,
    /// the filesystem is unmounted and the given session ends.
    pub unsafe fn new<FS: Filesystem + Send + 'a>(se: Session<FS>) -> io::Result<BackgroundSession<'a>> {
        let mountpoint = se.mountpoint().to_path_buf();
        let alive = Arc::new(AtomicBool::new(true));
        let last_error_code = Arc::new(Mutex::new(None));
        let alive_for_thread = Arc::clone(&alive);
        let last_error_for_thread = Arc::clone(&last_error_code);
        let guard = scoped(move || {
            struct SessionThreadState(Arc<AtomicBool>);
            impl Drop for SessionThreadState {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _state = SessionThreadState(alive_for_thread);
            let mut se = se;
            let out = se.run();
            if let Err(ref err) = out {
                if let Ok(mut slot) = last_error_for_thread.lock() {
                    *slot = err.raw_os_error();
                }
            }
            out
        });
        Ok(BackgroundSession {
            mountpoint: mountpoint,
            guard: guard,
            alive: alive,
            last_error_code: last_error_code,
        })
    }

    /// Return whether the background session thread is still running.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Return the last raw OS error code reported by the session loop.
    pub fn last_error_code(&self) -> Option<i32> {
        self.last_error_code.lock().ok().and_then(|slot| *slot)
    }
}

impl<'a> Drop for BackgroundSession<'a> {
    fn drop(&mut self) {
        info!("Unmounting {}", self.mountpoint.display());
        // Unmounting the filesystem will eventually end the session loop,
        // drop the session and hence end the background thread.
        match channel::unmount(&self.mountpoint) {
            Ok(()) => (),
            Err(err) => error!("Failed to unmount {}: {}", self.mountpoint.display(), err),
        }
    }
}

// replace with #[derive(Debug)] if Debug ever gets implemented for
// thread_scoped::JoinGuard
impl<'a> fmt::Debug for BackgroundSession<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "BackgroundSession {{ mountpoint: {:?}, guard: JoinGuard<()> }}", self.mountpoint)
    }
}
