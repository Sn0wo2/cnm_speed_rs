use anyhow::{Context, Result};
use flate2::{write::GzEncoder, Compression};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_log::LogTracer;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_DIR: &str = "data/logs";
const LATEST_LOG: &str = "latest.log";
const LOG_EXT: &str = "log";
const LOG_GZ_EXT: &str = "log.gz";

pub struct LoggerManager {
    _file_guard: WorkerGuard,
    pub current_log_path: PathBuf,
}

struct MinecraftTime;

impl FormatTime for MinecraftTime {
    fn format_time(&self, writer: &mut Writer<'_>) -> std::fmt::Result {
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        write!(
            writer,
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        )
    }
}

impl LoggerManager {
    pub fn init() -> Result<Self> {
        let log_dir = Path::new(LOG_DIR);
        fs::create_dir_all(log_dir).context("creating log directory")?;

        Self::rotate_latest_log(log_dir).context("rotating latest.log")?;

        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let current_log_name = format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.{}",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            LOG_EXT
        );
        let current_log_path = log_dir.join(&current_log_name);

        let file_appender = tracing_appender::rolling::never(log_dir, &current_log_name);
        let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

        Self::create_latest_symlink(log_dir, &current_log_path)
            .context("creating latest.log symlink")?;

        let _ = LogTracer::init();

        let file_layer = fmt::Layer::new()
            .with_writer(file_writer)
            .with_ansi(false)
            .with_level(true)
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .with_timer(MinecraftTime)
            .event_format(
                fmt::format()
                    .with_timer(MinecraftTime)
                    .with_level(true)
                    .with_target(false)
                    .compact(),
            );

        tracing_subscriber::registry()
            .with(file_layer)
            .try_init()
            .context("initializing tracing subscriber")?;

        let manager = Self {
            _file_guard: file_guard,
            current_log_path,
        };

        manager
            .compress_old_logs()
            .context("compressing legacy plain logs")?;

        tracing::info!(
            "Logger initialized at {}",
            Self::normalize_path_for_log(&manager.current_log_path)
        );
        Ok(manager)
    }

    fn normalize_path_for_log(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn rotate_latest_log(log_dir: &Path) -> Result<()> {
        let latest_path = log_dir.join(LATEST_LOG);
        if !latest_path.exists() {
            return Ok(());
        }

        if let Ok(target) = fs::read_link(&latest_path) {
            let source = if target.is_absolute() {
                target
            } else {
                log_dir.join(target)
            };
            if source.exists() {
                let archive_name = Self::archive_name_for(&source)?;
                let archive_path = log_dir.join(archive_name);
                Self::compress_file(&source, &archive_path)
                    .with_context(|| format!("compressing {}", source.display()))?;
                fs::remove_file(&source)
                    .with_context(|| format!("removing old plain log {}", source.display()))?;
            }
        }

        fs::remove_file(&latest_path)
            .with_context(|| format!("removing {}", latest_path.display()))?;
        Ok(())
    }

    fn create_latest_symlink(log_dir: &Path, current_log: &Path) -> Result<()> {
        let latest_path = log_dir.join(LATEST_LOG);
        if latest_path.exists() {
            fs::remove_file(&latest_path)
                .with_context(|| format!("removing stale symlink {}", latest_path.display()))?;
        }

        let relative = current_log.strip_prefix(log_dir).unwrap_or(current_log);

        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(relative, &latest_path)
                .with_context(|| format!("creating symlink {}", latest_path.display()))?;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(relative, &latest_path)
                .with_context(|| format!("creating symlink {}", latest_path.display()))?;
        }

        Ok(())
    }

    fn archive_name_for(source: &Path) -> Result<String> {
        if let Some(stem) = source.file_stem().and_then(|v| v.to_str()) {
            return Ok(format!("{}.{}", stem, LOG_GZ_EXT));
        }

        let metadata = fs::metadata(source)
            .with_context(|| format!("reading metadata for {}", source.display()))?;
        let modified = metadata.modified().context("reading modified time")?;
        let dt: OffsetDateTime = modified.into();
        Ok(format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.{}",
            dt.year(),
            u8::from(dt.month()),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            LOG_GZ_EXT
        ))
    }

    fn compress_file(source: &Path, target: &Path) -> Result<()> {
        let source_file =
            File::open(source).with_context(|| format!("open source {}", source.display()))?;
        let mut reader = BufReader::new(source_file);
        let target_file =
            File::create(target).with_context(|| format!("create target {}", target.display()))?;
        let mut encoder = GzEncoder::new(BufWriter::new(target_file), Compression::default());
        io::copy(&mut reader, &mut encoder).context("streaming bytes into gzip encoder")?;
        encoder.finish().context("finalizing gzip stream")?;
        Ok(())
    }

    pub fn compress_old_logs(&self) -> Result<()> {
        let log_dir = Path::new(LOG_DIR);
        for entry in fs::read_dir(log_dir).context("reading log directory")? {
            let path = entry.context("reading log directory entry")?.path();
            if path == self.current_log_path {
                continue;
            }
            if path.file_name().and_then(|v| v.to_str()) == Some(LATEST_LOG) {
                continue;
            }
            if path.extension().and_then(|v| v.to_str()) != Some(LOG_EXT) {
                continue;
            }

            let gz_path = path.with_extension(LOG_GZ_EXT);
            if gz_path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("removing duplicate plain log {}", path.display()))?;
                continue;
            }

            Self::compress_file(&path, &gz_path)
                .with_context(|| format!("compressing old log {}", path.display()))?;
            fs::remove_file(&path)
                .with_context(|| format!("removing old plain log {}", path.display()))?;
        }
        Ok(())
    }
}
