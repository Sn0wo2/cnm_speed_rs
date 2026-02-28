use simplelog::*;
use std::fs::File;

pub fn init_logging() {
    let log_path = "runtime.log";
    let file = match File::create(log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create {}: {}", log_path, e);
            return;
        }
    };

    let cfg = Config::default();
    if let Err(e) = WriteLogger::init(LevelFilter::Debug, cfg, file) {
        eprintln!("Logger init failed: {}", e);
    }
}
