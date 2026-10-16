//! InciWeb incident index, name matching and incident-page reading.
//!
//! The index (`api/map_data`, active incidents) supplies candidate pages; a perimeter is
//! linked only after its page's "Date of Origin" (or last-change date) shows the page
//! describes this season's fire rather than an archived one with the same name. Page
//! reading is a small, bounded tag scanner over recorded InciWeb markup: when it finds
//! nothing usable the result is "no link", never a failed perimeter snapshot.

use jiff::{SignedDuration, Timestamp, civil::Date, tz::TimeZone};
use lavender_domain::FirePerimeter;
use lavender_domain::text::clean;
use serde::Deserialize;

use crate::providers::{ProviderError, lenient, parse_rows};

pub const SOURCE_NAME: &str = "InciWeb";
pub const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_INDEX_ROWS: usize = 5_000;
/// A fire's InciWeb page is created around its start; a matching page whose origin is
/// earlier than discovery by more than this is an earlier season's same-name incident.
const ORIGIN_BEFORE_DISCOVERY: SignedDuration = SignedDuration::from_hours(60 * 24);
/// Without a discovery date, the page must have changed recently.
const CHANGED_WITHIN: SignedDuration = SignedDuration::from_hours(180 * 24);

/// One active InciWeb incident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: u32,
    pub title: String,
    /// Dispatch unit code, for example `ORPRD`; its first two letters are the state.
    pub unit: String,
    /// The page's path segment under `incident-information/`, for example `orprd-rowe-creek-complex`.
    pub slug: String,
    /// The title as names are compared, worked out once rather than for every fire.
    name_key: String,
}

impl IndexEntry {
    #[must_use]
    pub fn new(id: u32, title: String, unit: String, slug: String) -> Self {
        let name_key = normalize_name(&title);
        Self { id, title, unit, slug, name_key }
    }
}

/// What an incident page says about when it started and last changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFacts {
    pub origin: Option<Timestamp>,
    pub changed: Option<Timestamp>,
}

/// InciWeb sends ids as strings; numbers are accepted too.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawId {
    Text(String),
    Number(u32),
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<RawId>,
    #[serde(default, deserialize_with = "lenient")]
    title: Option<String>,
    #[serde(rename = "field_unit_code", default, deserialize_with = "lenient")]
    unit: Option<String>,
    #[serde(rename = "urlPath", default, deserialize_with = "lenient")]
    url_path: Option<String>,
    #[serde(rename = "field_active", default, deserialize_with = "lenient")]
    active: Option<String>,
}

/// Reads the incident index; malformed or inactive rows are skipped and counted.
///
/// # Errors
/// Rejects an oversized document or one that is not a JSON array of bounded length.
pub fn parse_index(body: &[u8]) -> Result<(Vec<IndexEntry>, usize), ProviderError> {
    let invalid = || ProviderError::parse(SOURCE_NAME, &"index is not a bounded JSON array");
    if body.len() > MAX_INDEX_BYTES {
        return Err(invalid());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|_| invalid())?;
    if rows.len() > MAX_INDEX_ROWS {
        return Err(invalid());
    }
    let (raw, mut dropped) = parse_rows::<RawEntry>(rows);
    let mut entries = Vec::with_capacity(raw.len());
    for row in raw {
        match entry(row) {
            Some(e) => entries.push(e),
            None => dropped += 1,
        }
    }
    Ok((entries, dropped))
}

fn entry(row: RawEntry) -> Option<IndexEntry> {
    if row.active.as_deref().is_some_and(|a| a != "1") {
        return None;
    }
    let id = match row.id? {
        RawId::Number(n) => n,
        RawId::Text(t) if (1..=9).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit()) => {
            t.parse().ok()?
        }
        RawId::Text(_) => return None,
    };
    let title = clean(row.title.as_deref()?, 200);
    let unit = row
        .unit
        .filter(|u| (2..=10).contains(&u.len()) && u.bytes().all(|b| b.is_ascii_alphanumeric()))?;
    let slug = row.url_path.as_deref()?.strip_prefix('/')?;
    let valid_slug = (1..=120).contains(&slug.len())
        && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    (valid_slug && !title.is_empty()).then(|| IndexEntry::new(id, title, unit, slug.to_owned()))
}

/// Lower case, single spaces, without a leading year ("2026 Coleman Creek") or a trailing
/// "Fire" ("Coyote Fire"): InciWeb titles carry both inconsistently; WFIGS names neither.
fn normalize_name(name: &str) -> String {
    let collapsed = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let without_year = match collapsed.split_once(' ') {
        Some((year, rest))
            if year.len() == 4
                && (year.starts_with("19") || year.starts_with("20"))
                && year.bytes().all(|b| b.is_ascii_digit()) =>
        {
            rest
        }
        _ => collapsed.as_str(),
    };
    without_year.strip_suffix(" fire").unwrap_or(without_year).to_owned()
}

/// The index entry for one name. The index is nationwide, so a name match counts only
/// when the dispatch unit's state agrees with the fire's (same-name fires burn in
/// different states); without a state only an unambiguous match counts. Same-state
/// duplicates resolve to the newest incident.
fn resolve<'a>(index: &'a [IndexEntry], name: &str, state: Option<&str>) -> Option<&'a IndexEntry> {
    let wanted = normalize_name(name);
    if wanted.is_empty() {
        return None;
    }
    let mut candidates: Vec<&IndexEntry> = index.iter().filter(|e| e.name_key == wanted).collect();
    let state_code = state
        .and_then(|s| s.strip_prefix("US-"))
        .filter(|code| code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()));
    match state_code {
        Some(code) => {
            candidates.retain(|e| e.unit.get(..2).is_some_and(|u| u.eq_ignore_ascii_case(code)));
        }
        None if candidates.len() > 1 => return None,
        None => {}
    }
    candidates.into_iter().max_by_key(|e| e.id)
}

/// The index entry for a perimeter: its own name first, then its complex's (InciWeb
/// often tracks the managing complex rather than each member fire).
#[must_use]
pub fn find_entry<'a>(index: &'a [IndexEntry], fire: &FirePerimeter) -> Option<&'a IndexEntry> {
    let state = fire.state.as_deref();
    fire.name
        .as_deref()
        .and_then(|name| resolve(index, name, state))
        .or_else(|| fire.complex_name.as_deref().and_then(|name| resolve(index, name, state)))
}

/// Whether a matched page plausibly describes the current fire. Fails closed when the
/// page gives no usable dates.
#[must_use]
pub fn is_current(facts: &PageFacts, discovered: Option<Timestamp>, now: Timestamp) -> bool {
    if let (Some(discovered), Some(origin)) = (discovered, facts.origin) {
        return discovered
            .checked_sub(ORIGIN_BEFORE_DISCOVERY)
            .is_ok_and(|earliest| origin >= earliest);
    }
    facts.changed.is_some_and(|changed| now.duration_since(changed) <= CHANGED_WITHIN)
}

/// Reads an incident page's "Date of Origin" and its last-change date (`og:updated_time`,
/// or the `dcterms.date` meta that 2026 pages carry instead).
///
/// # Errors
/// Rejects an oversized page or one with neither date.
pub fn parse_page(html: &[u8]) -> Result<PageFacts, ProviderError> {
    if html.len() > MAX_PAGE_BYTES {
        return Err(ProviderError::parse(SOURCE_NAME, &"incident page is too large"));
    }
    let html = String::from_utf8_lossy(html);
    let mut facts = PageFacts { origin: None, changed: None };
    let mut page_date = None;
    // The text of the header cell being read, and whether the current row is the
    // "Date of Origin" row.
    let mut header: Option<String> = None;
    let mut in_origin_row = false;
    for tag in Tags::new(&html) {
        if (tag.is("th") || tag.is("td") || tag.is("tr"))
            && let Some(label) = header.take()
        {
            let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
            in_origin_row = label.eq_ignore_ascii_case("Date of Origin");
        }
        if let Some(label) = header.as_mut()
            && label.len() < 200
        {
            // Text inside inline elements of the header cell, such as a <span>.
            label.push_str(tag.text);
        }
        if tag.closing {
            in_origin_row &= !tag.is("tr");
        } else if tag.is("th") {
            header = Some(tag.text.to_owned());
            in_origin_row = false;
        } else if tag.is("meta") {
            let content = tag.attribute("content");
            if tag.attribute("property") == Some("og:updated_time") {
                facts.changed = facts.changed.or_else(|| content.and_then(timestamp));
            } else if tag.attribute("name") == Some("dcterms.date") {
                page_date = page_date.or_else(|| content.and_then(date));
            }
        } else if tag.is("time") && in_origin_row && facts.origin.is_none() {
            facts.origin = tag.attribute("datetime").and_then(timestamp);
        }
    }
    facts.changed = facts.changed.or(page_date);
    if facts.origin.is_none() && facts.changed.is_none() {
        return Err(ProviderError::parse(SOURCE_NAME, &"incident page has no incident dates"));
    }
    Ok(facts)
}

fn plausible(stamp: Timestamp) -> Option<Timestamp> {
    (1990..=2100).contains(&stamp.to_zoned(TimeZone::UTC).year()).then_some(stamp)
}

/// RFC 3339 with an offset, for example `2026-08-01T17:45:28Z` or `…-04:00`.
fn timestamp(value: &str) -> Option<Timestamp> {
    (value.len() <= 40).then(|| value.trim().parse().ok()).flatten().and_then(plausible)
}

/// A calendar date (`2026-09-28`), read as midnight UTC.
fn date(value: &str) -> Option<Timestamp> {
    if value.len() > 16 {
        return None;
    }
    let day: Date = value.trim().parse().ok()?;
    day.to_zoned(TimeZone::UTC).ok().map(|z| z.timestamp()).and_then(plausible)
}

/// One start or end tag and the text that follows it up to the next tag.
struct Tag<'a> {
    name: &'a str,
    closing: bool,
    attributes: &'a str,
    text: &'a str,
}

impl<'a> Tag<'a> {
    fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// An attribute's value, quoted or not; names compare without case. Every pass
    /// consumes a name or an `=value`, so the loop always ends.
    fn attribute(&self, wanted: &str) -> Option<&'a str> {
        let mut rest = self.attributes;
        loop {
            rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
            if rest.is_empty() {
                return None;
            }
            let name_end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
                .unwrap_or(rest.len());
            let name = &rest[..name_end];
            rest = rest[name_end..].trim_start();
            let mut value = "";
            if let Some(after) = rest.strip_prefix('=') {
                let after = after.trim_start();
                let (found, remainder) = match after.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        let inner = &after[1..];
                        let end = inner.find(quote)?;
                        (&inner[..end], &inner[end + 1..])
                    }
                    _ => after.split_at(
                        after.find(|c: char| c.is_ascii_whitespace()).unwrap_or(after.len()),
                    ),
                };
                value = found;
                rest = remainder;
            }
            if !name.is_empty() && name.eq_ignore_ascii_case(wanted) {
                return Some(value);
            }
        }
    }
}

/// Iterates over a document's tags, skipping comments, declarations and the contents of
/// `script` and `style` elements.
struct Tags<'a> {
    rest: &'a str,
}

impl<'a> Tags<'a> {
    fn new(html: &'a str) -> Self {
        Self { rest: html }
    }

    fn skip_past(&mut self, marker: &str) {
        self.rest = self.rest.find(marker).map_or("", |i| &self.rest[i + marker.len()..]);
    }

    /// Moves to the closing tag of a raw-text element such as `script`.
    fn skip_raw_text(&mut self, name: &str) {
        let mut offset = 0;
        while let Some(found) = self.rest[offset..].find("</") {
            let start = offset + found;
            let candidate = self.rest.get(start + 2..start + 2 + name.len());
            if candidate.is_some_and(|c| c.eq_ignore_ascii_case(name)) {
                self.rest = &self.rest[start..];
                return;
            }
            offset = start + 2;
        }
        self.rest = "";
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = Tag<'a>;

    fn next(&mut self) -> Option<Tag<'a>> {
        loop {
            let start = self.rest.find('<')?;
            self.rest = &self.rest[start..];
            if self.rest.starts_with("<!--") {
                self.skip_past("-->");
                continue;
            }
            if self.rest.starts_with("<!") || self.rest.starts_with("<?") {
                self.skip_past(">");
                continue;
            }
            let body = &self.rest[1..];
            let closing = body.starts_with('/');
            let body = if closing { &body[1..] } else { body };
            let name_len = body.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(body.len());
            if name_len == 0 {
                // A '<' in text, not a tag.
                self.rest = &self.rest[1..];
                continue;
            }
            let (name, after_name) = body.split_at(name_len);
            let end = tag_end(after_name)?;
            let attributes = &after_name[..end];
            let remainder = &after_name[end + 1..];
            let text_len = remainder.find('<').unwrap_or(remainder.len());
            let text = &remainder[..text_len];
            self.rest = &remainder[text_len..];
            if !closing
                && (name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style"))
            {
                self.skip_raw_text(name);
            }
            return Some(Tag { name, closing, attributes, text });
        }
    }
}

/// The index of the `>` that ends a tag, ignoring any inside quoted attribute values. A
/// quote opens a value only straight after `=`, as in HTML, so an apostrophe elsewhere
/// cannot swallow the rest of the page.
fn tag_end(after_name: &str) -> Option<usize> {
    let mut quote = None;
    let mut after_equals = false;
    for (i, c) in after_name.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None if c == '>' => return Some(i),
            None if after_equals && (c == '"' || c == '\'') => quote = Some(c),
            None => {}
        }
        if !c.is_ascii_whitespace() {
            after_equals = quote.is_none() && c == '=';
        }
    }
    None
}
