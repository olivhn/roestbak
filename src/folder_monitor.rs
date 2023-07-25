use std::error::Error;
use std::ffi::{CStr, CString, OsStr};
use std::io::Error as IoError;
use std::mem;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

#[derive(Debug)]
pub enum FolderEvent {
    Added(PathBuf),
    Removed(PathBuf),
    AttributesChanged(PathBuf),
    EventQueueOverflowed,
}

pub struct FolderMonitor {
    inotify_fd: OwnedFd,
    folder_path: PathBuf,
}

impl FolderMonitor {
    pub fn new(folder: &Path) -> Result<FolderMonitor, SetupError> {
        let inotify_fd = create_inotify_fd()
            .map_err(|source| SetupError::CouldNotCreateFileDescriptor { source })?;
        add_inotify_folder_watch(inotify_fd.as_fd(), folder)
            .map_err(|source| SetupError::CouldNotAddWatch { source })?;

        let monitor = FolderMonitor {
            inotify_fd,
            folder_path: folder.to_path_buf(),
        };

        Ok(monitor)
    }

    pub fn process_filesystem_events(
        &self,
        mut block: impl FnMut(FolderEvent) -> (),
    ) -> Result<(), ProcessingError> {
        // Reading from inotify is a bit peculiar: for each event, the buffer will contain a `libc::inotify_event`
        // structure, optionally followed by a variable length character string for the associated filename.
        // Consequently, we have to read into a byte buffer, rather than a buffer of `libc::inotify_event`
        // structures.
        //
        // ⚠️ Because of this, extra attention is required to avoid unaligned reads. The approach taken below is
        // to copy each event into a local `libc::inotify_event` variable. As an alternative, the example code
        // in inotify(7) enables pointing directly into the buffer by arranging for it to have a proper alignment.

        const INOTIFY_EVENT_BASESIZE: usize = mem::size_of::<libc::inotify_event>();

        // The buffer should be larger than `sizeof(struct inotify_event) + NAME_MAX + 1` so that it can store at
        // least one event (NAME_MAX is presently defined to be 255).
        const BUFFER_SIZE: usize = 4096;

        let mut buffer = [0u8; BUFFER_SIZE];
        let mut offset: usize = 0;

        let bytes_read = unsafe {
            libc::read(
                self.inotify_fd.as_raw_fd(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };

        if bytes_read < 0 {
            let error = std::io::Error::last_os_error();

            if error
                .raw_os_error()
                .is_some_and(|code| code == libc::EAGAIN)
            {
                return Ok(());
            } else {
                return Err(ProcessingError::CouldNotReadFromFileDescriptor { source: error });
            }
        }

        let bytes_read = bytes_read as usize;

        while offset < bytes_read {
            let inotify_event = unsafe {
                let mut event = MaybeUninit::<libc::inotify_event>::uninit();
                assert!(offset + INOTIFY_EVENT_BASESIZE <= buffer.len());
                ptr::copy_nonoverlapping(
                    buffer.as_ptr().add(offset),
                    event.as_mut_ptr() as *mut u8,
                    INOTIFY_EVENT_BASESIZE,
                );
                event.assume_init()
            };

            // For reference, at present the kernel will queue up to 16384 events.
            if inotify_event.mask & libc::IN_Q_OVERFLOW != 0 {
                block(FolderEvent::EventQueueOverflowed);
            }

            let filename_field_length = usize::try_from(inotify_event.len).unwrap();

            if filename_field_length > 0 {
                let file_path = || {
                    let filename_field_offset = offset + INOTIFY_EVENT_BASESIZE;

                    assert!(filename_field_offset + filename_field_length <= buffer.len());

                    let filename_field_ptr = unsafe {
                        buffer.as_ptr().add(filename_field_offset) as *const libc::c_char
                    };

                    // The filename may be padded for alignment reasons, but the padding bytes should all be
                    // NUL characters.
                    assert!(unsafe { *filename_field_ptr.add(filename_field_length - 1) } == b'\0');

                    let file_name = unsafe { CStr::from_ptr(filename_field_ptr) };
                    let file_name = OsStr::from_bytes(file_name.to_bytes());

                    self.folder_path.join(Path::new(file_name))
                };

                let folder_event =
                    if inotify_event.mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                        Some(FolderEvent::Added(file_path()))
                    } else if inotify_event.mask & (libc::IN_DELETE | libc::IN_MOVED_FROM) != 0 {
                        Some(FolderEvent::Removed(file_path()))
                    } else if (inotify_event.mask & libc::IN_ATTRIB) != 0 {
                        Some(FolderEvent::AttributesChanged(file_path()))
                    } else {
                        None
                    };

                if let Some(folder_event) = folder_event {
                    block(folder_event);
                }
            };

            offset += INOTIFY_EVENT_BASESIZE + filename_field_length;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum SetupError {
    CouldNotCreateFileDescriptor { source: IoError },
    CouldNotAddWatch { source: IoError },
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetupError::CouldNotCreateFileDescriptor { source } => source,
            SetupError::CouldNotAddWatch { source } => source,
        })
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            SetupError::CouldNotCreateFileDescriptor { source: _ } => {
                "Could not create inotify file descriptor."
            }
            SetupError::CouldNotAddWatch { source: _ } => "Could not add inotify folder watch.",
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum ProcessingError {
    CouldNotReadFromFileDescriptor { source: IoError },
}

impl Error for ProcessingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ProcessingError::CouldNotReadFromFileDescriptor { source } => Some(source),
        }
    }
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            ProcessingError::CouldNotReadFromFileDescriptor { source: _ } => {
                "Read from inotify file descriptor failed."
            }
        };

        write!(f, "{}", description)
    }
}

fn create_inotify_fd() -> Result<OwnedFd, IoError> {
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd == -1 {
        Err(IoError::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn add_inotify_folder_watch(fd: BorrowedFd<'_>, folder: &Path) -> Result<(), IoError> {
    let folder = CString::new(folder.as_os_str().as_bytes()).unwrap();

    const WATCH_MASK: u32 = libc::IN_CREATE
        | libc::IN_MOVED_TO
        | libc::IN_ATTRIB
        | libc::IN_DELETE
        | libc::IN_MOVED_FROM
        | libc::IN_ONLYDIR;

    let result = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), folder.as_ptr(), WATCH_MASK) };

    if result == -1 {
        Err(IoError::last_os_error())
    } else {
        Ok(())
    }
}
