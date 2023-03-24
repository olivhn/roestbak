use crate::logging::SimpleLogger;
use std::process;

mod logging;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::install()?;

    log::info!("Starting roestbak service with PID {}.", process::id());

    Ok(())
}
