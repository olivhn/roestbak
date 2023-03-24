use log::{Level, Log, Metadata, Record, SetLoggerError};

pub struct SimpleLogger;

impl SimpleLogger {
    pub fn install() -> Result<(), SetLoggerError> {
        log::set_boxed_logger(Box::new(SimpleLogger))?;
        log::set_max_level(HARDCODED_MAX_LEVEL.to_level_filter());
        Ok(())
    }
}

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= HARDCODED_MAX_LEVEL
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "{} - {} - {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

const HARDCODED_MAX_LEVEL: Level = Level::Info;
