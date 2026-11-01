//! Background TCP client daemon streaming raw Mode-S AVR hex and Beast binary frames
//! into [`LocalReceiverEngine`].
//!
//! Validates target endpoints against SSRF rules (loopback or RFC1918 private IPv4 only),
//! maintains bounded buffer limits to prevent memory exhaustion, and reconnects with
//! exponential backoff and jitter.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use lavender_core::providers::local_receivers::{
    BeastMessageType, LocalReceiverEngine, is_local_receiver_host, is_safe_local_ip,
    parse_avr_stream, parse_beast_stream,
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

/// Maximum bounded buffer capacity in bytes for inbound socket streams.
pub const MAX_STREAM_BUFFER_CAPACITY: usize = 64 * 1024;

/// Initial reconnect backoff delay.
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(500);

/// Maximum reconnect backoff delay.
pub const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Read chunk size for socket operations.
const READ_CHUNK_SIZE: usize = 4096;

/// Local receiver raw stream format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalReceiverStreamFormat {
    /// ASCII AVR hex format (`*<hex>;\n` or `@<timestamp><hex>;\n`), standard port 30002.
    Avr,
    /// Beast binary format (`0x1a <type> <timestamp> <signal> <payload>`), standard port 30005.
    Beast,
}

/// Validated endpoint configuration for a local receiver TCP stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalReceiverTcpEndpoint {
    /// Hostname or IP address of the SDR receiver tap.
    pub host: String,
    /// TCP port number.
    pub port: u16,
    /// Expected stream encoding format.
    pub format: LocalReceiverStreamFormat,
}

impl LocalReceiverTcpEndpoint {
    /// Parses an endpoint configuration string.
    ///
    /// Accepts formats such as:
    /// - `127.0.0.1:30002` (inferred AVR from port 30002)
    /// - `127.0.0.1:30005` (inferred Beast from port 30005)
    /// - `127.0.0.1:30002=avr` or `127.0.0.1:30005=beast`
    /// - `avr://127.0.0.1:30002` or `beast://127.0.0.1:30005`
    /// - `tcp://127.0.0.1:30002` or `tcp://127.0.0.1:30005?format=beast`
    ///
    /// # Errors
    /// Returns an error string if host or port is missing or invalid, or if the host
    /// fails SSRF validation.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("empty endpoint string".to_owned());
        }

        let (explicit_format, without_scheme) = if let Some(rest) = trimmed.strip_prefix("avr://") {
            (Some(LocalReceiverStreamFormat::Avr), rest)
        } else if let Some(rest) = trimmed.strip_prefix("beast://") {
            (Some(LocalReceiverStreamFormat::Beast), rest)
        } else if let Some(rest) = trimmed.strip_prefix("tcp://") {
            (None, rest)
        } else {
            (None, trimmed)
        };

        let (target, format_suffix) = if let Some((t, f)) = without_scheme.split_once('=') {
            (t.trim(), Some(f.trim()))
        } else if let Some((t, query)) = without_scheme.split_once('?') {
            let fmt = query.strip_prefix("format=").unwrap_or(query);
            (t.trim(), Some(fmt.trim()))
        } else {
            (without_scheme.trim(), None)
        };

        let (host_str, port_str) = if target.starts_with('[') {
            let Some(end_bracket) = target.find(']') else {
                return Err("missing closing bracket in IPv6 address".to_owned());
            };
            let host_part = &target[1..end_bracket];
            let after = &target[end_bracket + 1..];
            let Some(colon) = after.strip_prefix(':') else {
                return Err("missing port after IPv6 address".to_owned());
            };
            (host_part, colon)
        } else {
            let Some((h, p)) = target.rsplit_once(':') else {
                return Err("endpoint must contain host:port".to_owned());
            };
            (h, p)
        };

        let host = host_str.trim().to_owned();
        if host.is_empty() {
            return Err("empty host".to_owned());
        }

        let port = port_str
            .trim()
            .parse::<u16>()
            .map_err(|e| format!("invalid port '{port_str}': {e}"))?;

        if !is_local_receiver_host(&host) {
            return Err(format!(
                "host '{host}' is not an allowed local receiver target (must be loopback, RFC1918, localhost, or *.local)"
            ));
        }

        let format = if let Some(f) = explicit_format {
            f
        } else if let Some(suffix) = format_suffix {
            match suffix.to_ascii_lowercase().as_str() {
                "avr" => LocalReceiverStreamFormat::Avr,
                "beast" => LocalReceiverStreamFormat::Beast,
                other => {
                    return Err(format!("unknown format '{other}': expected 'avr' or 'beast'"));
                }
            }
        } else if port == 30002 {
            LocalReceiverStreamFormat::Avr
        } else {
            LocalReceiverStreamFormat::Beast
        };

        Ok(Self { host, port, format })
    }
}

/// Parses a comma-separated list of endpoint specifications. Invalid entries are skipped with a warning.
#[must_use]
pub fn parse_tcp_endpoints(raw: &str) -> Vec<LocalReceiverTcpEndpoint> {
    let mut endpoints = Vec::new();
    for entry in raw.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        match LocalReceiverTcpEndpoint::parse(trimmed) {
            Ok(ep) => endpoints.push(ep),
            Err(e) => {
                tracing::warn!(entry = %trimmed, error = %e, "ignoring invalid local receiver TCP endpoint");
            }
        }
    }
    endpoints
}

async fn sleep_with_backoff(backoff: &mut Duration, cancel: &CancellationToken) {
    let jitter_ms = fastrand::u64(0..=250);
    let sleep_duration = *backoff + Duration::from_millis(jitter_ms);
    *backoff = (*backoff * 2).min(MAX_BACKOFF);

    tokio::select! {
        () = cancel.cancelled() => {}
        () = tokio::time::sleep(sleep_duration) => {}
    }
}

/// Runs a single TCP worker connecting to `endpoint`, unpacking Mode-S frames, and streaming them into `engine`.
#[allow(clippy::too_many_lines)]
pub async fn run_tcp_worker(
    endpoint: LocalReceiverTcpEndpoint,
    engine: Arc<LocalReceiverEngine>,
    cancel: CancellationToken,
) {
    let mut backoff = INITIAL_BACKOFF;

    while !cancel.is_cancelled() {
        if !is_local_receiver_host(&endpoint.host) {
            tracing::error!(
                host = %endpoint.host,
                "refusing connection: host failed SSRF check"
            );
            return;
        }

        let addr_target = format!("{}:{}", endpoint.host, endpoint.port);
        let addrs = match tokio::net::lookup_host(&addr_target).await {
            Ok(iter) => iter.collect::<Vec<SocketAddr>>(),
            Err(e) => {
                tracing::warn!(
                    target = %addr_target,
                    error = %e,
                    "local receiver DNS lookup failed"
                );
                sleep_with_backoff(&mut backoff, &cancel).await;
                continue;
            }
        };

        let safe_addrs: Vec<SocketAddr> =
            addrs.into_iter().filter(|a| is_safe_local_ip(a.ip())).collect();

        if safe_addrs.is_empty() {
            tracing::error!(
                target = %addr_target,
                "SSRF safety violation: host resolved to non-local address; terminating worker"
            );
            return;
        }

        let mut stream = match tokio::net::TcpStream::connect(&safe_addrs[..]).await {
            Ok(s) => {
                tracing::info!(
                    host = %endpoint.host,
                    port = endpoint.port,
                    format = ?endpoint.format,
                    "connected to local receiver TCP socket"
                );
                s
            }
            Err(e) => {
                tracing::debug!(
                    target = %addr_target,
                    error = %e,
                    "failed to connect to local receiver socket"
                );
                sleep_with_backoff(&mut backoff, &cancel).await;
                continue;
            }
        };

        // Reset backoff on successful connection
        backoff = INITIAL_BACKOFF;

        let mut buffer = Vec::with_capacity(READ_CHUNK_SIZE);
        let mut read_buf = [0_u8; READ_CHUNK_SIZE];

        loop {
            tokio::select! {
                () = cancel.cancelled() => {
                    tracing::info!(
                        host = %endpoint.host,
                        port = endpoint.port,
                        "local receiver worker stopping on cancellation"
                    );
                    return;
                }
                res = stream.read(&mut read_buf) => {
                    match res {
                        Ok(0) => {
                            tracing::warn!(
                                host = %endpoint.host,
                                port = endpoint.port,
                                "local receiver TCP connection closed by peer"
                            );
                            break;
                        }
                        Ok(n) => {
                            if buffer.len() + n > MAX_STREAM_BUFFER_CAPACITY {
                                let excess = (buffer.len() + n).saturating_sub(MAX_STREAM_BUFFER_CAPACITY);
                                let drain_len = excess.min(buffer.len());
                                buffer.drain(0..drain_len);
                            }
                            buffer.extend_from_slice(&read_buf[..n]);

                            match endpoint.format {
                                LocalReceiverStreamFormat::Avr => {
                                    let frames = parse_avr_stream(&mut buffer);
                                    let now = Timestamp::now();
                                    for frame in frames {
                                        let _ = engine.ingest_raw_mode_s_frame(&frame, now);
                                    }
                                }
                                LocalReceiverStreamFormat::Beast => {
                                    let packets = parse_beast_stream(&mut buffer);
                                    let now = Timestamp::now();
                                    for packet in packets {
                                        if packet.msg_type == BeastMessageType::ModeSShort
                                            || packet.msg_type == BeastMessageType::ModeSLong
                                        {
                                            let _ = engine.ingest_raw_mode_s_frame(&packet.payload, now);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                host = %endpoint.host,
                                port = endpoint.port,
                                error = %e,
                                "read error from local receiver TCP socket"
                            );
                            break;
                        }
                    }
                }
            }
        }

        sleep_with_backoff(&mut backoff, &cancel).await;
    }
}

/// Daemon managing background TCP stream connections to local SDR receivers.
#[derive(Debug)]
pub struct LocalReceiverDaemon {
    cancel: CancellationToken,
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl LocalReceiverDaemon {
    /// Spawns background workers for the provided endpoints.
    #[must_use]
    pub fn spawn(
        endpoints: Vec<LocalReceiverTcpEndpoint>,
        engine: &Arc<LocalReceiverEngine>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let mut handles = Vec::with_capacity(endpoints.len());

        for endpoint in endpoints {
            let cancel_clone = cancel.clone();
            let engine_clone = Arc::clone(engine);
            let handle = tokio::spawn(async move {
                run_tcp_worker(endpoint, engine_clone, cancel_clone).await;
            });
            handles.push(handle);
        }

        Self { cancel, _handles: handles }
    }

    /// Signals all workers to shut down.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

impl Drop for LocalReceiverDaemon {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Spawns a daemon from `LOCAL_RECEIVER_TCP_ENDPOINTS` environment variable if present.
#[must_use]
pub fn spawn_from_env(engine: &Arc<LocalReceiverEngine>) -> Option<LocalReceiverDaemon> {
    let raw = std::env::var("LOCAL_RECEIVER_TCP_ENDPOINTS").ok()?;
    let endpoints = parse_tcp_endpoints(&raw);
    if endpoints.is_empty() {
        return None;
    }
    Some(LocalReceiverDaemon::spawn(endpoints, engine))
}
