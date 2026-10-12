//! Server configuration: an optional TOML file, then `LAVENDER__SECTION__KEY` environment
//! overrides (ADR 0006), then validation that reports every problem at once.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use lavender_core::CoreConfig;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub server: ServerSettings,
    pub core: CoreConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerSettings {
    pub addr: SocketAddr,
    /// Host names (without port) the server answers. Requests for any other `Host` are
    /// refused before routing (DNS-rebinding protection).
    pub allowed_hosts: Vec<String>,
    /// The built frontend (`apps/web/dist`). Missing → API only.
    pub static_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_format: LogFormat,
    /// Proxies whose `X-Forwarded-For` is trusted for rate limiting.
    pub trusted_proxies: Vec<IpAddr>,
    /// Sustained `/api` requests per second per client, and the burst above it.
    pub rate_limit_per_s: u32,
    pub rate_limit_burst: u32,
    /// Live subscriptions one WebSocket connection may hold.
    pub max_subscriptions_per_connection: usize,
    /// Distinct feed regions one client (IPv4 address or IPv6 /64) may hold at once
    /// across its streams and snapshots, so one client cannot starve the rest. Behind a
    /// reverse proxy, set `trusted_proxies` or every user counts as the proxy.
    pub max_regions_per_client: usize,
    /// Concurrent WebSocket connections, in total and per client.
    pub max_connections: usize,
    pub max_connections_per_client: usize,
    /// A WebSocket that sends nothing (not even a pong to the server's pings, sent every
    /// 40 % of this) for this long is closed.
    pub ws_idle_timeout_s: u64,
    /// A WebSocket client that does not accept a frame within this long is closed.
    pub ws_send_timeout_s: u64,
    /// Inbound WebSocket messages per second per connection, and the burst above it.
    pub ws_messages_per_s: u32,
    pub ws_message_burst: u32,
    pub request_timeout_s: u64,
    /// Replays recorded provider documents from this directory instead of contacting live
    /// sources (end-to-end tests, offline demonstrations). Every snapshot then says the
    /// data is recorded. See `lavender_core::providers::fixture` for the layout.
    pub fixtures_dir: Option<PathBuf>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 7410)),
            allowed_hosts: ["localhost", "127.0.0.1", "[::1]"].map(str::to_owned).to_vec(),
            static_dir: PathBuf::from("apps/web/dist"),
            data_dir: PathBuf::from("var/data"),
            cache_dir: PathBuf::from("var/cache"),
            log_format: LogFormat::Text,
            trusted_proxies: Vec::new(),
            rate_limit_per_s: 20,
            rate_limit_burst: 60,
            max_subscriptions_per_connection: 32,
            max_regions_per_client: 32,
            max_connections: 512,
            max_connections_per_client: 16,
            ws_idle_timeout_s: 75,
            ws_send_timeout_s: 10,
            ws_messages_per_s: 10,
            ws_message_burst: 40,
            request_timeout_s: 30,
            fixtures_dir: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid configuration: {0}")]
    Parse(String),
    #[error("invalid configuration:\n  - {}", .0.join("\n  - "))]
    Invalid(Vec<String>),
}

/// Applies `LAVENDER__A__B=value` as `a.b = value` on the TOML document. Values parse as
/// TOML scalars or arrays when they can (`true`, `5`, `["a","b"]`) and are strings
/// otherwise.
fn apply_env_overrides(doc: &mut toml::Table, vars: impl Iterator<Item = (String, String)>) {
    for (key, raw) in vars {
        let Some(path) = key.strip_prefix("LAVENDER__") else { continue };
        let parts: Vec<String> = path.split("__").map(str::to_ascii_lowercase).collect();
        let Some((last, parents)) = parts.split_last() else { continue };
        let value = format!("v = {raw}")
            .parse::<toml::Table>()
            .ok()
            .and_then(|mut t| t.remove("v"))
            .unwrap_or(toml::Value::String(raw));
        let path: Vec<String> =
            parents.iter().cloned().chain(std::iter::once(last.clone())).collect();
        set_path(doc, &path, value);
    }
}

fn set_path(table: &mut toml::Table, path: &[String], value: toml::Value) {
    match path {
        [] => {}
        [last] => {
            table.insert(last.clone(), value);
        }
        [first, rest @ ..] => {
            let entry = table
                .entry(first.clone())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if !entry.is_table() {
                *entry = toml::Value::Table(toml::Table::new());
            }
            if let toml::Value::Table(next) = entry {
                set_path(next, rest, value);
            }
        }
    }
}

impl ServerConfig {
    /// Loads configuration from an optional file plus the process environment.
    ///
    /// # Errors
    /// Returns every problem found in the file, the overrides or the resulting values.
    pub fn load(path: Option<&std::path::Path>) -> Result<Self, ConfigError> {
        let text = match path {
            Some(p) => std::fs::read_to_string(p)
                .map_err(|source| ConfigError::Read { path: p.to_owned(), source })?,
            None => String::new(),
        };
        Self::from_sources(&text, std::env::vars())
    }

    /// As [`Self::load`], from explicit inputs (testable).
    ///
    /// # Errors
    /// Returns parse or validation errors.
    pub fn from_sources(
        toml_text: &str,
        env: impl Iterator<Item = (String, String)>,
    ) -> Result<Self, ConfigError> {
        let mut doc: toml::Table =
            toml_text.parse().map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?;
        apply_env_overrides(&mut doc, env);
        let config: Self = toml::Value::Table(doc)
            .try_into()
            .map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = self.core.validate().err().unwrap_or_default();
        let s = &self.server;
        if s.allowed_hosts.is_empty() {
            errors.push("server.allowed_hosts: list at least one host".to_owned());
        }
        if s.rate_limit_per_s == 0 || s.rate_limit_burst < s.rate_limit_per_s {
            errors.push("server.rate_limit: per_s must be > 0 and burst ≥ per_s".to_owned());
        }
        if !(1..=256).contains(&s.max_subscriptions_per_connection) {
            errors.push("server.max_subscriptions_per_connection: 1–256".to_owned());
        }
        if s.request_timeout_s == 0 {
            errors.push("server.request_timeout_s: must be positive".to_owned());
        }
        if s.max_regions_per_client == 0 {
            errors.push("server.max_regions_per_client: must be positive".to_owned());
        }
        if s.max_connections == 0
            || s.max_connections_per_client == 0
            || s.max_connections_per_client > s.max_connections
        {
            errors.push(
                "server.max_connections: both limits must be positive, per client ≤ total"
                    .to_owned(),
            );
        }
        if s.ws_idle_timeout_s == 0 || s.ws_send_timeout_s == 0 {
            errors.push(
                "server.ws_idle_timeout_s and ws_send_timeout_s: must be positive".to_owned(),
            );
        }
        if s.ws_messages_per_s == 0 || s.ws_message_burst < s.ws_messages_per_s {
            errors.push(
                "server.ws_messages_per_s: must be > 0 with ws_message_burst ≥ it".to_owned(),
            );
        }
        if errors.is_empty() { Ok(()) } else { Err(ConfigError::Invalid(errors)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn defaults_are_valid() {
        let c = ServerConfig::from_sources("", env(&[])).unwrap();
        assert_eq!(c.server.addr.port(), 7410);
    }

    #[test]
    fn env_overrides_win_over_the_file() {
        let file = "[server]\naddr = \"0.0.0.0:8080\"\n[core]\nlicence_mode = \"evaluation\"\n";
        let c = ServerConfig::from_sources(
            file,
            env(&[
                ("LAVENDER__SERVER__ADDR", "\"127.0.0.1:9000\""),
                ("LAVENDER__SERVER__ALLOWED_HOSTS", "[\"lavender.example\"]"),
                ("LAVENDER__CORE__LICENCE_MODE", "commercial"),
                ("LAVENDER__CORE__PROVIDERS__USGS__CADENCE_S", "120"),
                ("UNRELATED", "x"),
            ]),
        )
        .unwrap();
        assert_eq!(c.server.addr.port(), 9000);
        assert_eq!(c.server.allowed_hosts, vec!["lavender.example"]);
        assert_eq!(c.core.licence_mode, lavender_domain::LicenceMode::Commercial);
        assert_eq!(c.core.providers.usgs.cadence_s, 120);
    }

    #[test]
    fn reports_all_problems() {
        let err = ServerConfig::from_sources(
            "[server]\nallowed_hosts = []\nrate_limit_per_s = 0\n[core.providers.usgs]\nsummary = \"nope\"\n",
            env(&[]),
        )
        .unwrap_err();
        let ConfigError::Invalid(list) = err else { panic!("{err}") };
        assert_eq!(list.len(), 3, "{list:#?}");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(ServerConfig::from_sources("[server]\nadress = \"x\"\n", env(&[])).is_err());
    }
}
