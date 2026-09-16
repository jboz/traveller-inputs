use std::fs;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub type LogGuard = tracing_appender::non_blocking::WorkerGuard;

pub fn init_logging() -> anyhow::Result<LogGuard> {
    fs::create_dir_all("logs")?;
    let builder = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("traveller")
        .max_log_files(5)
        .build("logs")?;
    let (file_writer, guard) = tracing_appender::non_blocking(builder);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);
    let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("traveller=info".parse()?))
        .with(console_layer)
        .with(file_layer)
        .init();
    Ok(guard)
}

#[cfg(test)]
mod tests {
    #[test]
    fn logging_initializes() {
        let _guard = crate::logging::init_logging().expect("logging init");
        tracing::info!("log ok");
    }
}