use std::error::Error;
use std::io::Error as IoError;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

pub struct I2CDevice {
    device_fd: OwnedFd,
}

impl I2CDevice {
    pub fn new(device_file_path: &Path, slave_address: i32) -> Result<Self, SetupError> {
        let device_fd = ffi::open_i2c_device(device_file_path).map_err(|source| {
            SetupError::CouldNotOpenI2CDevice {
                path: device_file_path.to_path_buf(),
                source,
            }
        })?;
        ffi::set_slave_address(device_fd.as_fd(), slave_address).map_err(|source| {
            SetupError::CouldNotSetSlaveAddress {
                address: slave_address,
                source,
            }
        })?;

        Ok(Self { device_fd })
    }

    pub fn write_byte_data(&self, command: u8, value: u8) -> Result<(), WriteError> {
        ffi::i2c_smbus_write_byte_data(self.device_fd.as_fd(), command, value).map_err(|source| {
            WriteError::CouldNotWriteByteData {
                command,
                value,
                source,
            }
        })
    }

    pub fn read_byte_data(&self, command: u8) -> Result<u8, ReadError> {
        ffi::i2c_smbus_read_byte_data(self.device_fd.as_fd(), command)
            .map_err(|source| ReadError::CouldNotReadByteData { command, source })
    }
}

#[derive(Debug)]
pub enum SetupError {
    CouldNotOpenI2CDevice { path: PathBuf, source: IoError },
    CouldNotSetSlaveAddress { address: i32, source: IoError },
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetupError::CouldNotOpenI2CDevice { path: _, source } => source,
            SetupError::CouldNotSetSlaveAddress { address: _, source } => source,
        })
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            SetupError::CouldNotOpenI2CDevice { path, source: _ } => {
                format!("Could not open I2C device at {}.", path.display())
            }
            SetupError::CouldNotSetSlaveAddress { address, source: _ } => {
                format!("Could not set I2C slave address {:x}.", address)
            }
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum ReadError {
    CouldNotReadByteData { command: u8, source: IoError },
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            ReadError::CouldNotReadByteData { command: _, source } => source,
        })
    }
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            ReadError::CouldNotReadByteData { command, source: _ } => {
                format!("Could not read byte data using command {:x}.", command)
            }
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum WriteError {
    CouldNotWriteByteData {
        command: u8,
        value: u8,
        source: IoError,
    },
}

impl Error for WriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            WriteError::CouldNotWriteByteData {
                command: _,
                value: _,
                source,
            } => source,
        })
    }
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            WriteError::CouldNotWriteByteData {
                command,
                value,
                source: _,
            } => {
                format!("Could not write {:x} using command {:x}.", value, command)
            }
        };

        write!(f, "{}", description)
    }
}

mod ffi {
    use std::ffi::CString;
    use std::io::Error as IoError;
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
    use std::os::unix::prelude::OsStrExt;
    use std::path::Path;

    pub fn open_i2c_device(device_file_path: &Path) -> Result<OwnedFd, IoError> {
        let device_file_path = CString::new(device_file_path.as_os_str().as_bytes()).unwrap();

        let fd = unsafe { libc::open(device_file_path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };

        if fd == -1 {
            Err(IoError::last_os_error())
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }

    pub fn set_slave_address(device_fd: BorrowedFd<'_>, address: i32) -> Result<(), IoError> {
        const I2C_SLAVE_IOCTL_REQUEST: u64 = 0x0703;

        let result =
            unsafe { libc::ioctl(device_fd.as_raw_fd(), I2C_SLAVE_IOCTL_REQUEST, address) };

        if result < 0 {
            Err(IoError::last_os_error())
        } else {
            Ok(())
        }
    }

    const I2C_SMBUS_DATA_BLOCK_SIZE: usize = 34;

    // This matches the kernel's `i2c_smbus_data`.
    // This is a union in C, so it is represented here as a struct containing the largest possible union value.
    #[repr(C)]
    struct I2CSMBusData {
        block: [u8; I2C_SMBUS_DATA_BLOCK_SIZE],
    }

    impl I2CSMBusData {
        fn new() -> Self {
            Self {
                block: [0u8; I2C_SMBUS_DATA_BLOCK_SIZE],
            }
        }
    }

    // This matches the kernel's `i2c_smbus_ioctl_data`.
    #[repr(C)]
    struct I2CSMBusIoctlData {
        read_write: u8,
        command: u8,
        size: u32,
        data: *mut I2CSMBusData,
    }

    #[repr(u8)]
    enum I2CSMBusReadWrite {
        Read = 1,
        Write = 0,
    }

    impl I2CSMBusReadWrite {
        fn into_code(self) -> u8 {
            self as u8
        }
    }

    #[repr(u32)]
    enum I2CSMBusDataSize {
        ByteData = 2,
    }

    impl I2CSMBusDataSize {
        fn into_code(self) -> u32 {
            self as u32
        }
    }

    pub fn i2c_smbus_write_byte_data(
        device_fd: BorrowedFd<'_>,
        command: u8,
        value: u8,
    ) -> Result<(), IoError> {
        let mut data = I2CSMBusData::new();
        data.block[0] = value;

        i2c_smbus_access(
            device_fd,
            I2CSMBusReadWrite::Write,
            command,
            I2CSMBusDataSize::ByteData,
            &mut data,
        )?;

        Ok(())
    }

    pub fn i2c_smbus_read_byte_data(device_fd: BorrowedFd<'_>, command: u8) -> Result<u8, IoError> {
        let mut data = I2CSMBusData::new();

        i2c_smbus_access(
            device_fd,
            I2CSMBusReadWrite::Read,
            command,
            I2CSMBusDataSize::ByteData,
            &mut data,
        )?;

        Ok(data.block[0])
    }

    // This is based on `i2c_smbus_access` in `i2c-tools`.
    fn i2c_smbus_access(
        device_fd: BorrowedFd<'_>,
        read_write: I2CSMBusReadWrite,
        command: u8,
        data_size: I2CSMBusDataSize,
        data: *mut I2CSMBusData,
    ) -> Result<(), IoError> {
        const I2C_SMBUS_IOCTL_REQUEST: u64 = 0x0720;

        let mut i2c_smbus_ioctl_data = I2CSMBusIoctlData {
            read_write: read_write.into_code(),
            command,
            size: data_size.into_code(),
            data,
        };

        let result = unsafe {
            libc::ioctl(
                device_fd.as_raw_fd(),
                I2C_SMBUS_IOCTL_REQUEST,
                &mut i2c_smbus_ioctl_data,
            )
        };

        if result < 0 {
            Err(IoError::last_os_error())
        } else {
            Ok(())
        }
    }
}
