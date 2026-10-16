//! Disk storage for the launches provider, below the provider cache directory: the last
//! Launch Library 2 response, reused after a restart while it is younger than the cadence,
//! and the hourly request budget. Writes are atomic (temporary file, then rename) and reads
//! are size-capped; an unreadable file counts as absent.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use url::Url;

use super::budget::Budget;
use super::parsing::{LaunchPage, parse};

const MAX_RESPONSE_FILE_BYTES: u64 = 24 * 1024 * 1024;
const RESPONSE_SCHEMA: u8 = 2;
const MAX_BUDGET_FILE_BYTES: u64 = 64 * 1024;
const BUDGET_SCHEMA: u8 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResponseEnvelope {
    schema: u8,
    resource: String,
    body: String,
    downloaded_at: Timestamp,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BudgetEnvelope {
    schema: u8,
    budget: Budget,
}

fn launches_dir(dir: &Path) -> PathBuf {
    dir.join("launches")
}

fn response_path(dir: &Path) -> PathBuf {
    launches_dir(dir).join("launch-library-2.json")
}

fn budget_path(dir: &Path) -> PathBuf {
    launches_dir(dir).join("launch-library-2-budget.json")
}

/// The request without its rolling date bounds, which change on every request: a cached
/// response answers the same service, endpoint and query shape after a restart.
fn resource_identity(url: &Url) -> String {
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key != "net__gte" && key != "net__lte")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let mut identity = url.clone();
    identity.set_query(None);
    identity.query_pairs_mut().extend_pairs(pairs);
    identity.into()
}

/// The response saved for `url`'s resource and when it was downloaded.
pub(super) async fn load_response(
    dir: Option<&Path>,
    url: &Url,
) -> Option<(LaunchPage, Timestamp)> {
    let bytes = read_capped(&response_path(dir?), MAX_RESPONSE_FILE_BYTES).await?;
    let envelope: ResponseEnvelope = serde_json::from_slice(&bytes).ok()?;
    if envelope.schema != RESPONSE_SCHEMA || envelope.resource != resource_identity(url) {
        return None;
    }
    let page = parse(envelope.body.as_bytes()).ok()?;
    Some((page, envelope.downloaded_at))
}

pub(super) async fn save_response(
    dir: Option<&Path>,
    url: &Url,
    body: &[u8],
    downloaded_at: Timestamp,
) {
    let Some(dir) = dir else { return };
    let Ok(body) = std::str::from_utf8(body) else { return };
    let envelope = ResponseEnvelope {
        schema: RESPONSE_SCHEMA,
        resource: resource_identity(url),
        body: body.to_owned(),
        downloaded_at,
    };
    let Ok(bytes) = serde_json::to_vec(&envelope) else { return };
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RESPONSE_FILE_BYTES {
        return;
    }
    if let Err(error) = write_atomically(&response_path(dir), bytes).await {
        tracing::warn!(error = %error, "could not cache the Launch Library 2 response");
    }
}

/// The budget recorded by an earlier process, if any.
pub(super) async fn load_budget(dir: &Path) -> Option<Budget> {
    let bytes = read_capped(&budget_path(dir), MAX_BUDGET_FILE_BYTES).await?;
    let envelope: BudgetEnvelope = serde_json::from_slice(&bytes).ok()?;
    (envelope.schema == BUDGET_SCHEMA).then_some(envelope.budget)
}

pub(super) async fn save_budget(dir: &Path, budget: Budget) {
    let envelope = BudgetEnvelope { schema: BUDGET_SCHEMA, budget };
    let Ok(bytes) = serde_json::to_vec(&envelope) else { return };
    if let Err(error) = write_atomically(&budget_path(dir), bytes).await {
        // The in-memory budget still applies to this process.
        tracing::warn!(error = %error, "could not record the Launch Library 2 request budget");
    }
}

async fn read_capped(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut bytes).await.ok()?;
    (u64::try_from(bytes.len()).ok()? <= max_bytes).then_some(bytes)
}

async fn write_atomically(path: &Path, bytes: Vec<u8>) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("json.{:016x}.tmp", fastrand::u64(..)));
    let result = async {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rolling_window_is_not_part_of_the_cache_identity() {
        let a = Url::parse("https://ll.example/2.3.0/launches/?net__gte=1&net__lte=2&limit=100")
            .unwrap();
        let b = Url::parse("https://ll.example/2.3.0/launches/?net__gte=3&net__lte=4&limit=100")
            .unwrap();
        let other = Url::parse("https://ll.example/2.3.0/launches/?limit=10").unwrap();
        assert_eq!(resource_identity(&a), resource_identity(&b));
        assert_ne!(resource_identity(&a), resource_identity(&other));
    }

    #[tokio::test]
    async fn the_budget_round_trips_and_an_oversized_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut budget = Budget::default();
        budget.record_attempt(Timestamp::now());
        save_budget(dir.path(), budget.clone()).await;
        assert_eq!(load_budget(dir.path()).await, Some(budget));

        let huge = vec![b' '; usize::try_from(MAX_BUDGET_FILE_BYTES).unwrap() + 1];
        std::fs::write(budget_path(dir.path()), huge).unwrap();
        assert_eq!(load_budget(dir.path()).await, None);
    }
}
