pub mod convert;
pub mod download_context;
pub mod filenamify;
pub mod format_arg;
pub mod model;
pub mod nfo;
pub mod notify;
pub mod rule;
pub mod signal;
pub mod status;
pub mod validation;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::api::LogHelper;
use crate::config::CONFIG_DIR;

/// 线程安全的文件写入器，用于日志持久化
struct FileWriter {
    file: Arc<Mutex<std::fs::File>>,
}

impl FileWriter {
    fn new(file: std::fs::File) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
        }
    }
}

impl<'a> MakeWriter<'a> for FileWriter {
    type Writer = FileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        FileWriter {
            file: Arc::clone(&self.file),
        }
    }
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self.file.lock().unwrap();
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.flush()
    }
}

impl Clone for FileWriter {
    fn clone(&self) -> Self {
        FileWriter {
            file: Arc::clone(&self.file),
        }
    }
}

pub fn init_logger(log_level: &str, log_writer: Option<LogHelper>) {
    // 创建日志目录
    let log_dir = CONFIG_DIR.join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("无法创建日志目录 {}: {}", log_dir.display(), e);
    }

    // 创建日志文件路径
    let log_file = log_dir.join("bili-sync.log");

    // 尝试打开日志文件（追加模式）
    let file_writer = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(file) => {
            eprintln!("日志文件已创建: {}", log_file.display());
            Some(FileWriter::new(file))
        }
        Err(e) => {
            eprintln!("无法打开日志文件 {}: {}", log_file.display(), e);
            None
        }
    };

    // 使用 Registry 作为基础，然后添加多个 layer
    let registry = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::builder().parse_lossy(log_level));

    // 添加标准输出 layer（始终存在）
    let stdout_layer = fmt::layer()
        .compact()
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
            "%b %d %H:%M:%S".to_owned(),
        ));

    // 使用宏来统一构建 subscriber，避免类型不匹配
    macro_rules! build_subscriber {
        ($registry:expr, $stdout:expr, $file:expr, $ws:expr) => {
            match ($file, $ws) {
                (Some(file_w), Some(ws_w)) => {
                    let file_layer = fmt::layer()
                        .with_ansi(false)
                        .with_target(false)
                        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                            "%b %d %H:%M:%S".to_owned(),
                        ))
                        .with_writer(file_w);
                    let ws_layer = fmt::layer()
                        .with_ansi(false)
                        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                            "%b %d %H:%M:%S".to_owned(),
                        ))
                        .json()
                        .flatten_event(true)
                        .with_writer(ws_w);
                    $registry.with($stdout).with(file_layer).with(ws_layer)
                }
                (Some(file_w), None) => {
                    let file_layer = fmt::layer()
                        .with_ansi(false)
                        .with_target(false)
                        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                            "%b %d %H:%M:%S".to_owned(),
                        ))
                        .with_writer(file_w);
                    $registry.with($stdout).with(file_layer)
                }
                (None, Some(ws_w)) => {
                    let ws_layer = fmt::layer()
                        .with_ansi(false)
                        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                            "%b %d %H:%M:%S".to_owned(),
                        ))
                        .json()
                        .flatten_event(true)
                        .with_writer(ws_w);
                    $registry.with($stdout).with(ws_layer)
                }
                (None, None) => {
                    $registry.with($stdout)
                }
            }
        };
    }

    // 在每个分支中直接初始化，避免类型不匹配
    match (file_writer, log_writer) {
        (Some(file_w), Some(ws_w)) => {
            // 有文件日志和 WebSocket 日志
            let file_layer = fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                    "%b %d %H:%M:%S".to_owned(),
                ))
                .with_writer(file_w);
            let ws_layer = fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                    "%b %d %H:%M:%S".to_owned(),
                ))
                .json()
                .flatten_event(true)
                .with_writer(ws_w);
            registry
                .with(stdout_layer)
                .with(file_layer)
                .with(ws_layer)
                .try_init()
                .expect("初始化日志失败");
        }
        (Some(file_w), None) => {
            // 只有文件日志
            let file_layer = fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                    "%b %d %H:%M:%S".to_owned(),
                ))
                .with_writer(file_w);
            registry
                .with(stdout_layer)
                .with(file_layer)
                .try_init()
                .expect("初始化日志失败");
        }
        (None, Some(ws_w)) => {
            // 只有 WebSocket 日志
            let ws_layer = fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
                    "%b %d %H:%M:%S".to_owned(),
                ))
                .json()
                .flatten_event(true)
                .with_writer(ws_w);
            registry
                .with(stdout_layer)
                .with(ws_layer)
                .try_init()
                .expect("初始化日志失败");
        }
        (None, None) => {
            // 只有标准输出
            registry
                .with(stdout_layer)
                .try_init()
                .expect("初始化日志失败");
        }
    }
}
