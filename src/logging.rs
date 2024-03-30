use std::io::BufWriter;

use anyhow::bail;
use env_logger::Logger;

use crate::{built_info, LOG_DIR};

// this will be "info" in release mode and "debug" in debug mode
const DEFAULT_LOG_LEVEL: &'static str = {
    let mut i = 0;
    let debug_bytes = "debug".as_bytes();
    let built_profile_bytes = built_info::PROFILE.as_bytes();
    let debug_len = debug_bytes.len();
    let mut out = "debug";
    while i < debug_len {
        if built_profile_bytes[i] != debug_bytes[i] {
            // if profile str does not match 'debug'
            out = "info";
        }
        i += 1;
    }
    out
};

pub fn init_logging() {
    let _ = std::fs::create_dir_all(&*LOG_DIR);
    tokio::task::spawn_blocking(|| {
        let _ = purge_old_logs();
    });
    let mut use_file_logger = true;
    let logger = file_logger().unwrap_or_else(|_| {
        use_file_logger = false;
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or(DEFAULT_LOG_LEVEL),
        )
        .build()
    });

    let max_level = logger.filter();
    let r = log::set_boxed_logger(Box::new(logger));
    if r.is_ok() {
        log::set_max_level(max_level);
    }
    if !use_file_logger {
        log::warn!("Failed to create log file, using terminal instead");
    }
    if let Some(level) = max_level.to_level() {
        log::log!(level, "Logger initialized with 'max_level = {}'", level);
    }
}

fn purge_old_logs() -> anyhow::Result<()> {
    let log_files = std::fs::read_dir(&*LOG_DIR)?;
    for file in log_files {
        purge_old_log(file)?;
    }
    Ok(())
}

fn purge_old_log(
    dir_entry: Result<std::fs::DirEntry, std::io::Error>,
) -> anyhow::Result<()> {
    let file = dir_entry?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        bail!("Not a file")
    }
    let modified = metadata.modified()?;
    if std::time::SystemTime::now()
        .duration_since(modified)?
        .as_secs()
        > 60 * 60 * 24 * 14
    {
        std::fs::remove_file(file.path())?;
    }
    Ok(())
}

fn file_logger() -> anyhow::Result<Logger> {
    let writer = BufWriter::new(std::fs::File::create(LOG_DIR.join(format!(
        "log.{}.txt",
        chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S_%3f")
    )))?);
    let target = env_logger::Target::Pipe(Box::new(writer));
    Ok(env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(DEFAULT_LOG_LEVEL),
    )
    .target(target)
    .build())
}
