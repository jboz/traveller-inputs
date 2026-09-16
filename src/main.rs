use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args();
    let _bin = args.next();
    let config_path = match args.next() {
        Some(p) => p,
        None => "config.toml".to_string(),
    };
    let _guard = match traveller::logging::init_logging() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("init logging: {e}");
            return ExitCode::FAILURE;
        }
    };
    match traveller::run(&config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}