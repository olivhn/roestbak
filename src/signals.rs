use libc;
use std::error::Error;
use std::io::Error as IoError;
use std::mem;
use std::mem::MaybeUninit;
use std::ptr;

#[derive(Copy, Clone)]
pub enum SignalIntention {
    Terminate,
    ReloadConfiguration,
}

pub struct SignalManager {
    signal_fd: i32,
}

impl SignalManager {
    /// Install the signal manager.
    ///
    /// ⚠️ This will block the default handling of managed signals, even after the `SignalManager` instance is dropped.
    /// This is to avoid issues during a clean termination of the program. If the default action for SIGTERM would be restored
    /// before all cleanup code has had a chance to run, a second incoming SIGTERM could terminate the program prematurely.
    pub fn install() -> Result<SignalManager, InstallError> {
        let mask = create_signal_set(MANAGED_SIGNALS.iter().map(|mapping| mapping.0));

        block_signals(mask).map_err(|source| InstallError::CouldNotBlockSignals { source })?;

        let signal_fd = create_signal_fd(mask)
            .map_err(|source| InstallError::CouldNotCreateFileDescriptor { source })?;

        Ok(SignalManager { signal_fd })
    }

    pub fn next_signal(&self) -> Result<SignalIntention, ReceiveError> {
        loop {
            let signal_info = self.read_from_signal_fd()?;
            let received_signal = i32::try_from(signal_info.ssi_signo).expect(
                "Signals are defined as i32, but the field for them in signalfd_siginfo is a u32.",
            );

            if let Some(intention) = MANAGED_SIGNALS
                .iter()
                .find(|mapping| mapping.0 == received_signal)
                .map(|mapping| mapping.1)
            {
                return Ok(intention);
            }
        }
    }

    fn read_from_signal_fd(&self) -> Result<libc::signalfd_siginfo, ReceiveError> {
        const SIGNALFD_SIGINFO_SIZE: usize = mem::size_of::<libc::signalfd_siginfo>();

        unsafe {
            let mut signal_info: MaybeUninit<libc::signalfd_siginfo> = MaybeUninit::uninit();

            let bytes_read = libc::read(
                self.signal_fd,
                signal_info.as_mut_ptr() as *mut libc::c_void,
                SIGNALFD_SIGINFO_SIZE,
            );

            if bytes_read < 0 {
                return Err(ReceiveError::CouldNotReadFromFileDescriptor {
                    source: std::io::Error::last_os_error(),
                });
            }

            if bytes_read as usize != SIGNALFD_SIGINFO_SIZE {
                return Err(ReceiveError::InvalidReadFromFileDescriptor);
            }

            Ok(signal_info.assume_init())
        }
    }
}

impl Drop for SignalManager {
    fn drop(&mut self) {
        unsafe { libc::close(self.signal_fd) };
    }
}

const MANAGED_SIGNALS: [(i32, SignalIntention); 3] = [
    (libc::SIGTERM, SignalIntention::Terminate),
    (libc::SIGINT, SignalIntention::Terminate),
    (libc::SIGHUP, SignalIntention::ReloadConfiguration),
];

#[derive(Debug)]
pub enum InstallError {
    CouldNotBlockSignals { source: IoError },
    CouldNotCreateFileDescriptor { source: IoError },
}

impl Error for InstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            InstallError::CouldNotBlockSignals { source } => source,
            InstallError::CouldNotCreateFileDescriptor { source } => source,
        })
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            InstallError::CouldNotBlockSignals { source: _ } => {
                "Could not block signals while installing signal manager."
            }
            InstallError::CouldNotCreateFileDescriptor { source: _ } => {
                "Could not create signal file descriptor while installing signal manager."
            }
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum ReceiveError {
    CouldNotReadFromFileDescriptor { source: IoError },
    InvalidReadFromFileDescriptor,
}

impl Error for ReceiveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ReceiveError::CouldNotReadFromFileDescriptor { source } => Some(source),
            ReceiveError::InvalidReadFromFileDescriptor => None,
        }
    }
}

impl std::fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            ReceiveError::CouldNotReadFromFileDescriptor { source: _ } => {
                "Read from signal file descriptor failed."
            }
            ReceiveError::InvalidReadFromFileDescriptor => {
                "Read an invalid number of bytes from signal file descriptor."
            }
        };

        write!(f, "{}", description)
    }
}

fn create_signal_set<T>(signals: T) -> libc::sigset_t
where
    T: Iterator<Item = i32>,
{
    unsafe {
        let mut mask: MaybeUninit<libc::sigset_t> = MaybeUninit::uninit();
        libc::sigemptyset(mask.as_mut_ptr());
        for signal in signals {
            libc::sigaddset(mask.as_mut_ptr(), signal);
        }
        return mask.assume_init();
    }
}

fn block_signals(signal_set: libc::sigset_t) -> Result<(), IoError> {
    let result = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &signal_set, ptr::null_mut()) };
    if result != 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(())
    }
}

fn create_signal_fd(signal_set: libc::sigset_t) -> Result<i32, IoError> {
    let fd = unsafe { libc::signalfd(-1, &signal_set, 0) };
    if fd == -1 {
        Err(IoError::last_os_error())
    } else {
        Ok(fd)
    }
}
