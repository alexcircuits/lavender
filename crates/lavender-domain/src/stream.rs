//! Frames of the multiplexed live stream (ADR 0005). The WebSocket carries these as JSON
//! text frames; the Tauri adapter sends the `snapshot` and `error` payloads over channels.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{ApiError, FeedQuery, FeedSnapshot};

/// Client → server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ClientFrame {
    /// Start a subscription. `id` is chosen by the client and unique per connection.
    Subscribe {
        id: String,
        query: FeedQuery,
    },
    /// Change an existing subscription's query (for example a new viewport).
    Update {
        id: String,
        query: FeedQuery,
    },
    Unsubscribe {
        id: String,
    },
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ServerFrame {
    Snapshot {
        id: String,
        snapshot: FeedSnapshot,
    },
    /// A subscription-level error; `id` is absent for connection-level errors.
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        id: Option<String>,
        error: ApiError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeedId, FeedQuery};

    #[test]
    fn client_frames_use_op_tag() {
        let f: ClientFrame =
            serde_json::from_str(r#"{"op":"subscribe","id":"s1","query":{"feed":"earthquakes"}}"#)
                .unwrap();
        assert_eq!(
            f,
            ClientFrame::Subscribe { id: "s1".into(), query: FeedQuery::new(FeedId::Earthquakes) }
        );
        let bad = serde_json::from_str::<ClientFrame>(r#"{"op":"explode","id":"x"}"#);
        assert!(bad.is_err());
    }
}
