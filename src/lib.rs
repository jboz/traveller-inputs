#![allow(dead_code)]

pub mod clipboard;
pub mod coalescer;
pub mod config;
pub mod controller;
pub mod client;
pub mod focus;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod logging;
pub mod security;
pub mod transport;

use std::sync::Arc;

use config::Role;

pub async fn run_async(cfg: config::Config) -> anyhow::Result<()> {
    let expected = cfg
        .peer_fingerprint
        .as_deref()
        .map(security::parse_fingerprint)
        .transpose()?;
    let (handle, events) = match cfg.role {
        Role::Controller => transport::connect_loop(&cfg, expected).await?,
        Role::Client => transport::serve_loop(&cfg, expected).await?,
    };
    let bridge =
        clipboard::ClipboardBridge::new(clipboard::ArboardSource, &cfg.clipboard);
    match cfg.role {
        Role::Controller => {
            let capture = input::rdev_backend::RdevCapture::new()?;
            controller::run_controller(Arc::new(cfg), handle, events, capture, bridge).await
        }
        Role::Client => {
            let injector = input::rdev_backend::RdevInjector::new()?;
            client::run_client(handle, events, injector, bridge).await
        }
    }
}

pub fn run(config_path: &str) -> anyhow::Result<()> {
    let cfg = config::Config::load(config_path)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cfg))
}