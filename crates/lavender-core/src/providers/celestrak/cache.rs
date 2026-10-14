//! One bounded atomic envelope keeps the original document and its validators together.
//! Legacy raw OMM caches remain readable, with their modification time as validation age.

use super::{GroupData, MAX_BODY, parse};
use crate::fetch::CacheValidators;
use bytes::Bytes;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::io::AsyncReadExt;
use url::Url;

/// JSON-string escaping can at most double a valid JSON document's bytes. The small
/// remainder covers timestamps, resource URL and two validators (each capped at 512).
const MAX_CACHE_BYTES: usize = MAX_BODY * 2 + 8_192;
const SCHEMA: u8 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u8,
    resource: String,
    body: String,
    downloaded_at: Timestamp,
    validated_at: Timestamp,
    etag: Option<String>,
    last_modified: Option<String>,
}

fn cache_path(dir: &Path, group: &str) -> PathBuf {
    dir.join("celestrak").join(format!("{group}.json"))
}

pub(super) async fn load_cached_group(
    dir: Option<&Path>,
    group: &str,
    url: &Url,
) -> Option<GroupData> {
    let path = cache_path(dir?, group);
    let meta = tokio::fs::metadata(&path).await.ok()?;
    if meta.len() > u64::try_from(MAX_CACHE_BYTES).ok()? {
        return None;
    }
    // Re-check after reading: a concurrently replaced or growing file is never parsed
    // above the cap. take() also bounds allocation before that check.
    let file = tokio::fs::File::open(&path).await.ok()?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_CACHE_BYTES).ok()? + 1).read_to_end(&mut bytes).await.ok()?;
    if bytes.len() > MAX_CACHE_BYTES {
        return None;
    }
    let now = Timestamp::now();
    let (body, downloaded_at, validated_at, validators) =
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'[') {
            if bytes.len() > MAX_BODY {
                return None;
            }
            let modified = meta.modified().ok().unwrap_or(SystemTime::UNIX_EPOCH);
            let at = Timestamp::try_from(modified).ok()?;
            (Bytes::from(bytes), at, at, None)
        } else {
            let envelope: Envelope = serde_json::from_slice(&bytes).ok()?;
            if envelope.schema != SCHEMA
                || envelope.resource != url.as_str()
                || envelope.body.len() > MAX_BODY
            {
                return None;
            }
            let validators = match (&envelope.etag, &envelope.last_modified) {
                (None, None) => None,
                (etag, modified) => {
                    Some(CacheValidators::new(etag.as_deref(), modified.as_deref()).ok()?)
                }
            };
            (Bytes::from(envelope.body), envelope.downloaded_at, envelope.validated_at, validators)
        };
    if downloaded_at > validated_at || validated_at > now {
        return None;
    }
    let (elements, dropped) = parse(&body, group).ok()?;
    Some(GroupData { elements, dropped, body, downloaded_at, validated_at, validators })
}

pub(super) async fn save_cached_group(
    dir: Option<&Path>,
    group: &str,
    url: &Url,
    data: &GroupData,
) {
    let Some(dir) = dir else { return };
    let Ok(body) = std::str::from_utf8(&data.body) else { return };
    if body.len() > MAX_BODY || url.as_str().len() > 4_096 {
        return;
    }
    let envelope = Envelope {
        schema: SCHEMA,
        resource: url.as_str().to_owned(),
        body: body.to_owned(),
        downloaded_at: data.downloaded_at,
        validated_at: data.validated_at,
        etag: data.validators.as_ref().and_then(CacheValidators::etag).map(str::to_owned),
        last_modified: data
            .validators
            .as_ref()
            .and_then(CacheValidators::last_modified)
            .map(str::to_owned),
    };
    let Ok(bytes) = serde_json::to_vec(&envelope) else { return };
    if bytes.len() > MAX_CACHE_BYTES {
        return;
    }
    let path = cache_path(dir, group);
    let tmp = path.with_extension(format!("json.{:016x}.tmp", fastrand::u64(..)));
    let result = async {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, &path).await
    }
    .await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&tmp).await;
        tracing::warn!(group, error = %error, "could not cache CelesTrak group");
    }
}
