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

/// Point d'entrée applicatif. Chargé par main.rs.
pub fn run(_config_path: &str) -> anyhow::Result<()> {
    tracing::warn!("traveller: implémentation à venir (tâches suivantes)");
    Ok(())
}