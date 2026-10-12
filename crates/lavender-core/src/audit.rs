//! Audit hook points at the facade (product spec §4.3). The default sink does nothing;
//! enterprise deployments plug in a persistent one without touching callers.

use lavender_domain::FeedId;

/// Something worth auditing that passed through the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    Subscribed { feed: FeedId },
    Snapshot { feed: FeedId },
    Searched { chars: usize },
    SatelliteTrack { norad_id: u32 },
}

pub trait AuditSink: Send + Sync + std::fmt::Debug {
    fn record(&self, event: AuditEvent);
}

/// Discards events.
#[derive(Debug, Default)]
pub struct NoAudit;

impl AuditSink for NoAudit {
    fn record(&self, _event: AuditEvent) {}
}
