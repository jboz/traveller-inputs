use anyhow::{bail, Context};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Controller,
    Client,
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "controller" => Ok(Role::Controller),
            "client" => Ok(Role::Client),
            other => Err(serde::de::Error::custom(format!("role inconnu: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
}

impl<'de> Deserialize<'de> for Side {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "right" => Ok(Side::Right),
            "left" => Ok(Side::Left),
            other => Err(serde::de::Error::custom(format!("side inconnu: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Screen {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    pub enabled: bool,
    pub sync_interval_ms: u64,
    pub max_kb: usize,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self { enabled: true, sync_interval_ms: 300, max_kb: 4096 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KeysConfig {
    pub emergency_stop: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self { emergency_stop: "Ctrl+Alt+F10".into() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub role: Role,
    pub listen: Option<String>,
    pub peer_addr: Option<String>,
    pub screen: Screen,
    pub peer_screen: Screen,
    pub side: Side,
    #[serde(default)]
    pub peer_fingerprint: Option<String>,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    #[serde(default)]
    pub clipboard: ClipboardConfig,
    #[serde(default)]
    pub keys: KeysConfig,
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Config> {
        let raw = std::fs::read_to_string(path).with_context(|| format!("lecture config {path}"))?;
        let mut cfg: Config =
            toml::from_str(&raw).with_context(|| format!("parsing config {path}"))?;
        let base = Path::new(path).parent().unwrap_or(Path::new("."));
        cfg.cert_path = absolutize(base, &cfg.cert_path);
        cfg.key_path = absolutize(base, &cfg.key_path);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        match (&self.listen, &self.peer_addr) {
            (None, None) => bail!("config: il faut `listen` OU `peer_addr`"),
            (Some(_), Some(_)) => bail!("config: `listen` et `peer_addr` sont mutuellement exclusifs"),
            _ => {}
        }
        if self.screen.width == 0 || self.screen.height == 0 {
            bail!("config: `screen` doit avoir width et height > 0");
        }
        if self.peer_screen.width == 0 || self.peer_screen.height == 0 {
            bail!("config: `peer_screen` doit avoir width et height > 0");
        }
        if let Some(fp) = &self.peer_fingerprint {
            let ok = fp
                .strip_prefix("sha256:")
                .map(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                .unwrap_or(false);
            if !ok {
                bail!("config: `peer_fingerprint` attendu au format sha256:<64 hex>");
            }
        }
        Ok(())
    }
}

fn absolutize(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, toml: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(name);
        std::fs::write(&p, toml).unwrap();
        let path = p.to_str().unwrap().to_string();
        (dir, path)
    }

    fn base() -> &'static str {
        r#"role = "controller"
listen = "0.0.0.0:7722"
screen = { width = 1920, height = 1080 }
peer_screen = { width = 2560, height = 1440 }
side = "right"
cert_path = "certs/cert.pem"
key_path = "certs/key.pem"
"#
    }

    #[test]
    fn parse_valide() {
        let (_dir, p) = tmp("c.toml", base());
        let c = Config::load(&p).unwrap();
        assert_eq!(c.role, Role::Controller);
        assert_eq!(c.screen.width, 1920);
        assert_eq!(c.side, Side::Right);
        assert!(c.cert_path.is_absolute());
    }

    #[test]
    fn listen_et_peer_exclusifs() {
        let (_dir, p) = tmp("c.toml", &format!(r#"{base}peer_addr = "x:7722"
"#, base = base()));
        assert!(Config::load(&p).is_err());
    }

    #[test]
    fn dimensions_non_nulles() {
        let bad = base().replace("width = 1920", "width = 0");
        let (_dir, p) = tmp("c.toml", &bad);
        assert!(Config::load(&p).is_err());
    }

    #[test]
    fn fingerprint_format() {
        let (_dir, p) = tmp("c.toml", base());
        let mut c = Config::load(&p).unwrap();
        c.peer_fingerprint = Some("sha256:xyz".into());
        assert!(c.validate().is_err());
        c.peer_fingerprint = Some(format!("sha256:{}", "ab".repeat(32)));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn defauts_clipboard_keys() {
        let (_dir, p) = tmp("c.toml", base());
        let c = Config::load(&p).unwrap();
        assert!(c.clipboard.enabled);
        assert_eq!(c.clipboard.sync_interval_ms, 300);
        assert_eq!(c.keys.emergency_stop, "Ctrl+Alt+F10");
    }
}