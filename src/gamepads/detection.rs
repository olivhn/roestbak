use crate::folder_monitor::{
    FolderEvent, FolderMonitor, ProcessingError as FolderMonitorProcessingError,
    SetupError as FolderMonitorSetupError,
};
use once_cell::sync::Lazy;
use regex::bytes::Regex;
use std::collections::VecDeque;
use std::error::Error;
use std::fs;
use std::io::Error as IoError;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const GAMEPAD_DEVICE_FOLDER: &str = "/dev/input/";
static GAMEPAD_DEVICE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^js-evdev\d*$").unwrap());

pub struct GamepadDetector {
    gamepad_devices: VecDeque<PathBuf>,
    folder_monitor: FolderMonitor,
}

impl GamepadDetector {
    pub fn new() -> Result<GamepadDetector, SetupError> {
        // The order is important here: We should not risk missing out on events by scanning the file system
        // first and only setting up folder monitoring afterwards.

        let folder_monitor = FolderMonitor::new(Path::new(GAMEPAD_DEVICE_FOLDER))
            .map_err(|source| SetupError::CouldNotSetupFolderMonitor { source })?;

        let gamepad_devices = scan_for_gamepad_devices()
            .map_err(|source| SetupError::CouldNotScanForDeviceFiles { source })?;

        let gamepad_detector = GamepadDetector {
            gamepad_devices,
            folder_monitor,
        };

        Ok(gamepad_detector)
    }

    // 💁‍♂️ Calling this repeatedly will return each available device in turn.
    pub fn next_gamepad_device(&mut self) -> Option<&Path> {
        if self.gamepad_devices.len() > 1 {
            self.gamepad_devices.rotate_left(1);
        }

        self.gamepad_devices.front().map(|path| path.as_path())
    }

    pub fn process_updates(&mut self) -> Result<(), ProcessingError> {
        self.folder_monitor
            .process_filesystem_events(|event| {
                match event {
                    FolderEvent::Added(path) => {
                        if is_gamepad_device_file(&path) {
                            if !self.gamepad_devices.contains(&path) {
                                self.gamepad_devices.push_back(path);
                            }
                        }
                    }
                    FolderEvent::Removed(path) => {
                        if is_gamepad_device_file(&path) {
                            self.gamepad_devices.retain(|element| element != &path);
                        }
                    }
                    FolderEvent::AttributesChanged(_) => {
                        // A device file created by udev might—at least in certain cases—not yet be readable by
                        // us when we receive an `Added` event for it. When the permissions are fixed in a
                        // separate step we'll receive an `AttributesChanged` event for the device file.
                        //
                        // This is entirely ignored here, though: A read error on a device will not cause it to
                        // be removed from the list of detected devices. As long as the list is not empty, each
                        // device file can be tried periodically.
                    }
                    FolderEvent::EventQueueOverflowed => {
                        // Events may have been irretrievably lost in this case, so the only way to re-sync the 
                        // devices list would be to scan the filesystem again. However, we cannot make any 
                        // potentially blocking system calls in this context, so this is not an option. We'll 
                        // therefore just clear the devices list, meaning that an operator will have to reconnect 
                        // any gamepads for them to be detected again.
                        // 
                        // Note that this argument is entirely theoretical: The kernel will at present allow up 
                        // to 16384 events to be queued making an overflow quite unlikely. 

                        log::error!("Inotify event queue overflowed. The list of detected devices will be cleared.");
                        self.gamepad_devices.clear();
                    }
                }
            })
            .map_err(|source| ProcessingError::FolderMonitorCouldNotProcessEvents { source })
    }
}

#[derive(Debug)]
pub enum SetupError {
    CouldNotSetupFolderMonitor { source: FolderMonitorSetupError },
    CouldNotScanForDeviceFiles { source: IoError },
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetupError::CouldNotSetupFolderMonitor { source } => source,
            SetupError::CouldNotScanForDeviceFiles { source } => source,
        })
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            SetupError::CouldNotSetupFolderMonitor { source: _ } => {
                "Could not setup folder monitor while setting up gamepad detector."
            }
            SetupError::CouldNotScanForDeviceFiles { source: _ } => {
                "Could not scan for device files while setting up gamepad detector."
            }
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum ProcessingError {
    FolderMonitorCouldNotProcessEvents {
        source: FolderMonitorProcessingError,
    },
}

impl Error for ProcessingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ProcessingError::FolderMonitorCouldNotProcessEvents { source } => Some(source),
        }
    }
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            ProcessingError::FolderMonitorCouldNotProcessEvents { source: _ } => {
                "Folder monitor encountered issue processing events."
            }
        };

        write!(f, "{}", description)
    }
}

fn scan_for_gamepad_devices() -> Result<VecDeque<PathBuf>, IoError> {
    let iterator = fs::read_dir(Path::new(GAMEPAD_DEVICE_FOLDER))?;

    let mut devices = VecDeque::<PathBuf>::new();

    for entry in iterator {
        let path = entry?.path();

        if is_gamepad_device_file(&path) {
            devices.push_back(path);
        }
    }

    Ok(devices)
}

fn is_gamepad_device_file(path: &Path) -> bool {
    !path.is_dir()
        && path
            .file_name()
            .map(|name| name.as_bytes())
            .is_some_and(|name| GAMEPAD_DEVICE_REGEX.is_match(name))
}
