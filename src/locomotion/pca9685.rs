use std::{error::Error, path::Path, time::Duration};

use crate::i2c::{self, I2CDevice};

// The datasheet is available at: https://cdn-shop.adafruit.com/datasheets/PCA9685.pdf.

pub struct PCA9685Driver {
    i2c_device: I2CDevice,
}

impl PCA9685Driver {
    pub fn new(i2c_device_file_path: &Path, pwm_frequency: u32) -> Result<Self, SetupError> {
        let i2c_device = I2CDevice::new(i2c_device_file_path, I2C_BUS_ADDRESS)?;

        // This resets MODE1 and MODE2 to their default values. Setting the SLEEP bit will stop all PWM output.
        i2c_device.write_byte_data(REGISTER_MODE1, MODE1_ALLCALL_FLAG | MODE1_SLEEP_FLAG)?;
        i2c_device.write_byte_data(REGISTER_MODE2, MODE2_OUTDRV_FLAG)?;

        // The prescale can only be set while the SLEEP bit is set.
        let prescale = prescale_value_for_frequency(pwm_frequency);
        i2c_device.write_byte_data(REGISTER_PRESCALE, prescale)?;

        // After wake-up, a 500μs delay is required before configuring PWM outputs.
        i2c_device.write_byte_data(REGISTER_MODE1, MODE1_ALLCALL_FLAG)?;
        std::thread::sleep(Duration::from_micros(500));

        // The PWM outputs will remain reset after the sleep cycle, so the device should be in fresh start-up
        // state now. (While unneeded here, note for future reference that there is a RESTART functionality
        // that allows for restarting the PWM outputs after a sleep cycle.)

        Ok(Self { i2c_device })
    }

    pub fn set_pwm_on_percentage(&self, channel: u8, percentage: f64) -> Result<(), SetPWMError> {
        assert!(percentage >= 0.0);
        assert!(percentage <= 1.0);

        self.set_pwm(channel, 0, (percentage * 4095.0).round() as u16)
    }

    fn set_pwm(&self, channel: u8, on: u16, off: u16) -> Result<(), SetPWMError> {
        assert!(channel < 16);

        self.i2c_device
            .write_byte_data(REGISTER_LED0_ON_L + 4 * channel, (on & 0xFF) as u8)?;
        self.i2c_device
            .write_byte_data(REGISTER_LED0_ON_H + 4 * channel, (on >> 8) as u8)?;
        self.i2c_device
            .write_byte_data(REGISTER_LED0_OFF_L + 4 * channel, (off & 0xFF) as u8)?;
        self.i2c_device
            .write_byte_data(REGISTER_LED0_OFF_H + 4 * channel, (off >> 8) as u8)?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum SetupError {
    I2CWriteError { source: i2c::WriteError },
    I2CSetupError { source: i2c::SetupError },
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetupError::I2CWriteError { source } => source,
            SetupError::I2CSetupError { source } => source,
        })
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not set up PCA9685 device.")
    }
}

impl From<i2c::WriteError> for SetupError {
    fn from(value: i2c::WriteError) -> Self {
        SetupError::I2CWriteError { source: value }
    }
}

impl From<i2c::SetupError> for SetupError {
    fn from(value: i2c::SetupError) -> Self {
        SetupError::I2CSetupError { source: value }
    }
}

#[derive(Debug)]
pub enum SetPWMError {
    I2CWriteError { source: i2c::WriteError },
}

impl Error for SetPWMError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetPWMError::I2CWriteError { source } => source,
        })
    }
}

impl std::fmt::Display for SetPWMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not set PWM value on PCA9685 device.")
    }
}

impl From<i2c::WriteError> for SetPWMError {
    fn from(value: i2c::WriteError) -> Self {
        SetPWMError::I2CWriteError { source: value }
    }
}

const I2C_BUS_ADDRESS: i32 = 0x40;

const REGISTER_MODE1: u8 = 0x00;
const REGISTER_MODE2: u8 = 0x01;
const REGISTER_LED0_ON_L: u8 = 0x06;
const REGISTER_LED0_ON_H: u8 = 0x07;
const REGISTER_LED0_OFF_L: u8 = 0x08;
const REGISTER_LED0_OFF_H: u8 = 0x09;
const REGISTER_PRESCALE: u8 = 0xFE;

const MODE2_OUTDRV_FLAG: u8 = 0x04;

const MODE1_ALLCALL_FLAG: u8 = 0x01;
const MODE1_SLEEP_FLAG: u8 = 0x10;

fn prescale_value_for_frequency(pwm_frequency: u32) -> u8 {
    let internal_oscillator_frequency: f64 = 25000000.0;
    let pwm_frequency = pwm_frequency as f64;

    let prescale_value = (internal_oscillator_frequency / (4096.0 * pwm_frequency)).round() - 1.0;

    assert!(prescale_value >= 0x03 as f64);
    assert!(prescale_value <= 0xFF as f64);

    prescale_value as u8
}
