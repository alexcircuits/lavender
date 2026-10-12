//! `lavender-server` — serves the Lavender API, live stream and frontend.
//!
//! Configuration: `--config <file.toml>` (or `LAVENDER_CONFIG`), then `LAVENDER__*`
//! environment overrides. Provider keys come from `LAVENDER_*` secret variables
//! (see `lavender_core::secrets::SecretKey::env_var`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use lavender_core::providers::fixture::FixtureProvider;
use lavender_core::{EnvSecretStore, FileSettingsStore, LavenderCore};
use lavender_server::config::{LogFormat, ServerConfig};
use tracing_subscriber::EnvFilter;

fn config_path() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
        if let Some(p) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var_os("LAVENDER_CONFIG").map(PathBuf::from)
}

fn init_tracing(format: &LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Text => builder.init(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match ServerConfig::load(config_path().as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lavender-server: {e}");
            return ExitCode::from(2);
        }
    };
    init_tracing(&config.server.log_format);
    let mut builder = LavenderCore::builder(config.core.clone())
        .secrets(Arc::new(EnvSecretStore::from_env()))
        .settings(Arc::new(FileSettingsStore::new(config.server.data_dir.join("settings.json"))))
        .cache_dir(config.server.cache_dir.clone());
    if let Some(dir) = &config.server.fixtures_dir {
        tracing::warn!(dir = %dir.display(), "fixture mode: replaying recorded provider data, no live sources");
        builder = builder.providers(FixtureProvider::all(dir));
    }
    let core = match builder.build() {
        Ok(core) => core,
        Err(e) => {
            eprintln!("lavender-server: {e}");
            return ExitCode::from(2);
        }
    };
    if !config.server.static_dir.join("index.html").exists() {
        tracing::warn!(dir = %config.server.static_dir.display(), "frontend not built; serving the API only");
    }
    let listener = match tokio::net::TcpListener::bind(config.server.addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("lavender-server: cannot listen on {}: {e}", config.server.addr);
            return ExitCode::from(1);
        }
    };
    tracing::info!(addr = %config.server.addr, licence_mode = ?config.core.licence_mode, "Lavender is listening");
    let app = lavender_server::app(core.clone(), &config.server);
    let served = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await;
    core.shutdown();
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "server stopped with an error");
            ExitCode::from(1)
        }
    }
}
