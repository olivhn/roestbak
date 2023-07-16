use crate::logging::SimpleLogger;
use crate::signals::{SignalIntention, SignalManager};
use std::error::Error;
use std::process::{self, ExitCode};

mod logging;
mod runloop;
mod signals;

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

    loop {
        match signal_manager.next_signal()? {
            SignalIntention::Terminate => {
                log::info!("Received termination signal.");
                break;
            }
            SignalIntention::ReloadConfiguration => {
                log::info!("Ignoring configuration reload signal.");
            }
        }
    }

    Ok(())
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
