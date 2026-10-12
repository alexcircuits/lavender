use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether a source may be used in a product that is sold (ADR 0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CommercialUse {
    /// Open licence or public domain; attribution and usage policy suffice.
    Allowed,
    /// Usable commercially only with a paid plan, contract or written permission.
    RequiresAgreement,
    /// The licence forbids commercial use.
    NonCommercialOnly,
    /// No licence is published or it could not be verified.
    Unknown,
}

/// Whether a source carries privacy or ethics risk (cameras, licence-plate readers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Privacy {
    Normal,
    /// Off by default in every licence mode and shown with an in-product notice.
    Sensitive,
}

/// The licence a source is published under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Licence {
    pub name: String,
    pub url: String,
}

/// A data provider as registered in the core. The attribution panel, the inspector's
/// provenance row and the licence gate all read from this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceInfo {
    /// Stable identifier, for example `usgs-earthquakes`.
    pub id: String,
    pub name: String,
    pub homepage: String,
    /// The attribution text exactly as the provider mandates it.
    pub attribution: String,
    pub licence: Licence,
    pub commercial: CommercialUse,
    /// Whether the source needs a key or credentials to work.
    pub requires_key: bool,
    pub privacy: Privacy,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notes: Option<String>,
}

/// How strictly licences are enforced (ADR 0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LicenceMode {
    /// Every implemented source may be enabled; restrictions are shown, not enforced.
    #[default]
    Evaluation,
    /// Sources whose commercial use is not `Allowed` stay disabled unless acknowledged.
    Commercial,
}
