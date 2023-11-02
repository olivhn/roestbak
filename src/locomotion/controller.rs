use super::pca9685::{self, PCA9685Driver};
use std::{error::Error, path::Path};

#[derive(Debug, Copy, Clone)]
pub struct LocomotionCommand {
    // -1.0 for full reverse to 1.0 for full speed forward.
    throttle: f64,

    // -1.0 for steering maximally to the left to 1.0 for steering maximally to the right.
    direction: f64,
}

impl LocomotionCommand {
    pub fn new(throttle: f64, direction: f64) -> Self {
        assert!(throttle >= -1.0);
        assert!(throttle <= 1.0);
        assert!(direction >= -1.0);
        assert!(direction <= 1.0);

        Self {
            throttle,
            direction,
        }
    }

    pub fn get_throttle(&self) -> f64 {
        self.throttle
    }

    pub fn get_direction(&self) -> f64 {
        self.direction
    }
}

pub struct LocomotionController {
    pca9685_driver: PCA9685Driver,
}

impl LocomotionController {
    pub fn new() -> Result<Self, SetupError> {
        let pca9685_driver = PCA9685Driver::new(Path::new(I2C_DEVICE_FILE), PWM_FREQUENCY)
            .map_err(|source| SetupError::PCA9685SetupError { source })?;

        // This will initialize the ESC.
        pca9685_driver
            .set_pwm_on_percentage(PCA9685_THROTTLE_CHANNEL, PWM_CENTER_ON_PCT)
            .map_err(|source| SetupError::CouldNotInitializeESC { source })?;

        Ok(Self { pca9685_driver })
    }

    pub fn execute_command(&self, command: LocomotionCommand) -> Result<(), ExecuteCommandError> {
        self.pca9685_driver.set_pwm_on_percentage(
            PCA9685_THROTTLE_CHANNEL,
            locomotion_value_to_pwm_on_percentage(command.get_throttle()),
        )?;
        self.pca9685_driver.set_pwm_on_percentage(
            PCA9685_STEERING_CHANNEL,
            locomotion_value_to_pwm_on_percentage(command.get_direction()),
        )?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum SetupError {
    PCA9685SetupError { source: pca9685::SetupError },
    CouldNotInitializeESC { source: pca9685::SetPWMError },
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            SetupError::PCA9685SetupError { source } => source,
            SetupError::CouldNotInitializeESC { source } => source,
        })
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            SetupError::PCA9685SetupError { source: _ } => {
                format!("Locomotion controller initialization error.")
            }
            SetupError::CouldNotInitializeESC { source: _ } => {
                format!("Locomotion controller initialization error: Could not send initialization signal to ESC.")
            }
        };

        write!(f, "{}", description)
    }
}

#[derive(Debug)]
pub enum ExecuteCommandError {
    SetPWMError { source: pca9685::SetPWMError },
}

impl Error for ExecuteCommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            ExecuteCommandError::SetPWMError { source } => source,
        })
    }
}

impl std::fmt::Display for ExecuteCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Encountered issue executing locomotion command.")
    }
}

impl From<pca9685::SetPWMError> for ExecuteCommandError {
    fn from(value: pca9685::SetPWMError) -> Self {
        ExecuteCommandError::SetPWMError { source: value }
    }
}

const I2C_DEVICE_FILE: &str = "/dev/i2c-1";

const PCA9685_THROTTLE_CHANNEL: u8 = 0;
const PCA9685_STEERING_CHANNEL: u8 = 1;

const PWM_FREQUENCY: u32 = 50;

// 1ms, 1.5ms and 2ms per cycle.
const PWM_MIN_ON_PCT: f64 = 1.0 * (PWM_FREQUENCY as f64) / 1000.0;
const PWM_CENTER_ON_PCT: f64 = 1.5 * (PWM_FREQUENCY as f64) / 1000.0;
const PWM_MAX_ON_PCT: f64 = 2.0 * (PWM_FREQUENCY as f64) / 1000.0;

fn locomotion_value_to_pwm_on_percentage(value: f64) -> f64 {
    if value == 0.0 {
        PWM_CENTER_ON_PCT
    } else if value > 0.0 {
        PWM_CENTER_ON_PCT - ((PWM_CENTER_ON_PCT - PWM_MIN_ON_PCT) * value)
    } else {
        PWM_CENTER_ON_PCT + ((PWM_MAX_ON_PCT - PWM_CENTER_ON_PCT) * value.abs())
    }
}
