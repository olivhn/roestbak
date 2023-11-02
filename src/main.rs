use crate::gamepads::GamepadInputInterpreter;
use crate::locomotion::LocomotionController;
use crate::logging::SimpleLogger;
use crate::runloop::IterationOutcome;
use crate::signals::{SignalIntention, SignalManager};
use std::error::Error;
use std::process::{self, ExitCode};
use std::time::Duration;

mod folder_monitor;
mod gamepads;
mod i2c;
mod locomotion;
mod logging;
mod runloop;
mod signals;

const RUNLOOP_INTERVAL: Duration = Duration::from_millis(20);

fn main() -> ExitCode {
    match run_application() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{}", FatalErrorFormatter { error: &error });
            ExitCode::FAILURE
        }
    }
}

fn run_application() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::install()?;

    log::info!("Starting roestbak service with PID {}.", process::id());

    let signal_manager = SignalManager::install()?;
    let mut gamepad_input_interpreter = GamepadInputInterpreter::new()?;
    let locomotion_controller = LocomotionController::new()?;

    runloop::start_runloop(RUNLOOP_INTERVAL, || {
        if let Some(signal) = signal_manager.next_signal()? {
            match signal {
                SignalIntention::Terminate => {
                    log::info!("Received termination signal.");
                    return Ok(IterationOutcome::Conclude);
                }
                SignalIntention::ReloadConfiguration => {
                    log::info!("Ignoring configuration reload signal.");
                }
            }
        }

        let locomotion_command = gamepad_input_interpreter.process_input()?;
        locomotion_controller.execute_command(locomotion_command)?;

        Ok(IterationOutcome::KeepGoing)
    })
}

struct FatalErrorFormatter<'a> {
    error: &'a Box<dyn Error>,
}

impl<'a> std::fmt::Display for FatalErrorFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FATAL: {}", self.error)?;

        let mut next_source = self.error.source();
        while let Some(source) = next_source {
            write!(f, " - Caused by: {}", source)?;
            next_source = source.source();
        }

        Ok(())
    }
}
