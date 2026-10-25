//! In-memory deterministic entity resolution engine with bounded LRU capacity.

use std::collections::{HashMap, VecDeque};
use std::sync::{PoisonError, RwLock};

use lavender_domain::{
    Aircraft, CanonicalEntity, EntityIdentifier, EntityKind, EntityLink, FeedItems, FeedSnapshot,
    SatelliteElements, Vessel,
};

use crate::config::ResolutionConfig;
use crate::error::CoreError;

/// Normalizes an identifier scheme and value for consistent indexing and lookup.
#[must_use]
pub fn normalize_identifier(scheme: &str, value: &str) -> (String, String) {
    let s = scheme.trim().to_ascii_lowercase();
    let v = match s.as_str() {
        "icao24" | "hex" => value.trim().to_ascii_lowercase(),
        "callsign" | "registration" | "name" => value.trim().to_ascii_uppercase(),
        _ => value.trim().to_string(),
    };
    (s, v)
}

#[derive(Debug)]
struct ResolutionInner {
    entities: HashMap<String, (CanonicalEntity, u64)>,
    lru_order: VecDeque<(String, u64)>,
    alias_index: HashMap<(String, String), String>,
    links: VecDeque<EntityLink>,
    config: ResolutionConfig,
    seq: u64,
}

impl ResolutionInner {
    fn new(config: ResolutionConfig) -> Self {
        let max_entities = config.max_entities.max(1);
        let max_links = config.max_links.max(1);
        Self {
            entities: HashMap::with_capacity(max_entities.min(1024)),
            lru_order: VecDeque::with_capacity(max_entities.min(1024)),
            alias_index: HashMap::with_capacity(max_entities.saturating_mul(2).min(2048)),
            links: VecDeque::with_capacity(max_links.min(1024)),
            config,
            seq: 0,
        }
    }

    fn touch_entity(&mut self, id: &str) -> Option<&CanonicalEntity> {
        self.seq = self.seq.wrapping_add(1);
        let current_seq = self.seq;
        if let Some((entity, entry_seq)) = self.entities.get_mut(id) {
            *entry_seq = current_seq;
            self.lru_order.push_back((id.to_string(), current_seq));
            Some(entity)
        } else {
            None
        }
    }

    fn get_entity(&mut self, id: &str) -> Option<CanonicalEntity> {
        self.touch_entity(id).cloned()
    }

    fn resolve(&mut self, scheme: &str, value: &str) -> Option<CanonicalEntity> {
        let key = normalize_identifier(scheme, value);
        if key.0.is_empty() || key.1.is_empty() {
            return None;
        }
        let canonical_id = self.alias_index.get(&key)?.clone();
        self.get_entity(&canonical_id)
    }

    fn insert_entity(&mut self, entity: CanonicalEntity) {
        self.seq = self.seq.wrapping_add(1);
        let current_seq = self.seq;
        let id = entity.id.clone();

        for identifier in &entity.identifiers {
            let key = normalize_identifier(&identifier.scheme, &identifier.value);
            self.alias_index.insert(key, id.clone());
        }

        self.entities.insert(id.clone(), (entity, current_seq));
        self.lru_order.push_back((id, current_seq));
        self.evict_if_needed();
    }

    fn evict_if_needed(&mut self) {
        while self.entities.len() > self.config.max_entities {
            if let Some((candidate_id, candidate_seq)) = self.lru_order.pop_front() {
                if let Some((_, actual_seq)) = self.entities.get(&candidate_id)
                    && *actual_seq == candidate_seq
                    && let Some((evicted, _)) = self.entities.remove(&candidate_id)
                {
                    for id in evicted.identifiers {
                        let key = normalize_identifier(&id.scheme, &id.value);
                        if self.alias_index.get(&key) == Some(&candidate_id) {
                            self.alias_index.remove(&key);
                        }
                    }
                }
            } else {
                break;
            }
        }
        self.prune_links_if_needed();
    }

    fn prune_links_if_needed(&mut self) {
        while self.links.len() > self.config.max_links {
            self.links.pop_front();
        }
    }

    fn link_alias(
        &mut self,
        canonical_id: &str,
        identifier: &EntityIdentifier,
    ) -> Result<CanonicalEntity, CoreError> {
        let (s, v) = normalize_identifier(&identifier.scheme, &identifier.value);
        if s.is_empty() || v.is_empty() {
            return Err(CoreError::BadRequest(
                "identifier scheme and value cannot be empty".to_string(),
            ));
        }

        if !self.entities.contains_key(canonical_id) {
            return Err(CoreError::NotFound(format!(
                "canonical entity '{canonical_id}' not found"
            )));
        }

        let key = (s.clone(), v.clone());
        let normalized_id = EntityIdentifier::new(s.clone(), v.clone());

        if let Some(existing_canonical_id) = self.alias_index.get(&key)
            && existing_canonical_id != canonical_id
        {
            let existing_id_clone = existing_canonical_id.clone();
            if let Some((old_entity, _)) = self.entities.get_mut(&existing_id_clone) {
                old_entity.identifiers.retain(|i| {
                    normalize_identifier(&i.scheme, &i.value) != (s.clone(), v.clone())
                });
            }
        }

        let updated_entity = if let Some((entity, entry_seq)) = self.entities.get_mut(canonical_id)
        {
            self.seq = self.seq.wrapping_add(1);
            *entry_seq = self.seq;
            let current_seq = self.seq;
            self.lru_order.push_back((canonical_id.to_string(), current_seq));

            if !entity
                .identifiers
                .iter()
                .any(|i| normalize_identifier(&i.scheme, &i.value) == (s.clone(), v.clone()))
            {
                entity.identifiers.push(normalized_id);
            }

            entity.clone()
        } else {
            return Err(CoreError::NotFound(format!(
                "canonical entity '{canonical_id}' not found"
            )));
        };

        self.alias_index.insert(key, canonical_id.to_string());

        let now_ts = jiff::Timestamp::now().as_second();
        self.links.push_back(EntityLink {
            source_id: canonical_id.to_string(),
            target_id: format!("{s}:{v}"),
            relation: "alias_of".to_string(),
            confidence: 1.0,
            created_at: now_ts,
        });

        self.prune_links_if_needed();

        Ok(updated_entity)
    }

    fn split_alias(
        &mut self,
        canonical_id: &str,
        identifier: &EntityIdentifier,
    ) -> Result<CanonicalEntity, CoreError> {
        let (s, v) = normalize_identifier(&identifier.scheme, &identifier.value);
        let key = (s.clone(), v.clone());

        let (kind, first_seen, last_seen) = {
            let (entity, entry_seq) = self.entities.get_mut(canonical_id).ok_or_else(|| {
                CoreError::NotFound(format!("canonical entity '{canonical_id}' not found"))
            })?;

            let has_identifier = entity
                .identifiers
                .iter()
                .any(|i| normalize_identifier(&i.scheme, &i.value) == (s.clone(), v.clone()));

            if !has_identifier {
                return Err(CoreError::NotFound(format!(
                    "identifier '{}:{}' not associated with entity '{canonical_id}'",
                    identifier.scheme, identifier.value
                )));
            }

            if entity.identifiers.len() <= 1 {
                return Err(CoreError::BadRequest(format!(
                    "cannot split sole identifier from entity '{canonical_id}'"
                )));
            }

            entity
                .identifiers
                .retain(|i| normalize_identifier(&i.scheme, &i.value) != (s.clone(), v.clone()));

            self.seq = self.seq.wrapping_add(1);
            *entry_seq = self.seq;
            let current_seq = self.seq;
            self.lru_order.push_back((canonical_id.to_string(), current_seq));

            (entity.kind, entity.first_seen, entity.last_seen)
        };

        let new_id = format!("canonical:split:{s}:{v}");
        let new_entity = CanonicalEntity {
            id: new_id.clone(),
            kind,
            primary_label: v.clone(),
            identifiers: vec![EntityIdentifier::new(s.clone(), v.clone())],
            attributes: HashMap::new(),
            first_seen,
            last_seen,
            confidence: 1.0,
        };

        self.alias_index.insert(key, new_id.clone());

        let now_ts = jiff::Timestamp::now().as_second();
        self.links.push_back(EntityLink {
            source_id: new_id.clone(),
            target_id: format!("{s}:{v}"),
            relation: "alias_of".to_string(),
            confidence: 1.0,
            created_at: now_ts,
        });

        self.insert_entity(new_entity.clone());
        Ok(new_entity)
    }

    fn merge_entities(
        &mut self,
        target_id: &str,
        source_id: &str,
    ) -> Result<CanonicalEntity, CoreError> {
        if target_id == source_id {
            return self.get_entity(target_id).ok_or_else(|| {
                CoreError::NotFound(format!("canonical entity '{target_id}' not found"))
            });
        }

        if !self.entities.contains_key(target_id) {
            return Err(CoreError::NotFound(format!(
                "target canonical entity '{target_id}' not found"
            )));
        }

        let (source_entity, _) = self.entities.remove(source_id).ok_or_else(|| {
            CoreError::NotFound(format!("source canonical entity '{source_id}' not found"))
        })?;

        let updated_entity = {
            let (target_entity, entry_seq) = self.entities.get_mut(target_id).ok_or_else(|| {
                CoreError::NotFound(format!("target canonical entity '{target_id}' not found"))
            })?;

            self.seq = self.seq.wrapping_add(1);
            *entry_seq = self.seq;
            let current_seq = self.seq;
            self.lru_order.push_back((target_id.to_string(), current_seq));

            target_entity.first_seen = target_entity.first_seen.min(source_entity.first_seen);
            target_entity.last_seen = target_entity.last_seen.max(source_entity.last_seen);
            target_entity.confidence = target_entity.confidence.max(source_entity.confidence);

            for (k, v) in source_entity.attributes {
                target_entity.attributes.entry(k).or_insert(v);
            }

            for id in source_entity.identifiers {
                let key = normalize_identifier(&id.scheme, &id.value);
                if !target_entity
                    .identifiers
                    .iter()
                    .any(|existing| normalize_identifier(&existing.scheme, &existing.value) == key)
                {
                    target_entity.identifiers.push(id);
                }
            }
            target_entity.clone()
        };

        for id in &updated_entity.identifiers {
            let key = normalize_identifier(&id.scheme, &id.value);
            self.alias_index.insert(key, target_id.to_string());
        }

        let now_ts = jiff::Timestamp::now().as_second();
        self.links.push_back(EntityLink {
            source_id: target_id.to_string(),
            target_id: source_id.to_string(),
            relation: "merged_from".to_string(),
            confidence: 1.0,
            created_at: now_ts,
        });

        self.prune_links_if_needed();

        Ok(updated_entity)
    }

    fn get_entity_aliases(
        &mut self,
        canonical_id: &str,
    ) -> Result<Vec<EntityIdentifier>, CoreError> {
        let entity = self.get_entity(canonical_id).ok_or_else(|| {
            CoreError::NotFound(format!("canonical entity '{canonical_id}' not found"))
        })?;
        Ok(entity.identifiers)
    }

    fn get_entity_links(&self, entity_id: &str) -> Vec<EntityLink> {
        self.links
            .iter()
            .filter(|l| l.source_id == entity_id || l.target_id == entity_id)
            .cloned()
            .collect()
    }

    fn ingest_feed_snapshot(&mut self, snapshot: &FeedSnapshot) -> usize {
        let now_ts = snapshot.fetched_at.unwrap_or(snapshot.generated_at).as_second();
        let mut count = 0;
        match &snapshot.items {
            FeedItems::Aircraft(items) => {
                for ac in items {
                    self.ingest_aircraft(ac, now_ts);
                    count += 1;
                }
            }
            FeedItems::Vessels(items) => {
                for v in items {
                    self.ingest_vessel(v, now_ts);
                    count += 1;
                }
            }
            FeedItems::Satellites(items) => {
                for s in items {
                    self.ingest_satellite(s, now_ts);
                    count += 1;
                }
            }
            _ => {}
        }
        count
    }

    #[allow(clippy::too_many_lines)]
    fn ingest_aircraft(&mut self, ac: &Aircraft, now_ts: i64) {
        let hex = ac.hex.to_ascii_lowercase();
        let icao_id = EntityIdentifier::new("icao24", hex.clone());
        let reg_id = ac.registration.as_ref().and_then(|r| {
            let trimmed = r.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(EntityIdentifier::new("registration", trimmed.to_ascii_uppercase()))
            }
        });
        let cs_id = ac.callsign.as_ref().and_then(|c| {
            let trimmed = c.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(EntityIdentifier::new("callsign", trimmed.to_ascii_uppercase()))
            }
        });

        let mut candidate_ids = vec![icao_id.clone()];
        if let Some(r) = &reg_id {
            candidate_ids.push(r.clone());
        }
        if let Some(c) = &cs_id {
            candidate_ids.push(c.clone());
        }

        let mut resolved_canonical_id = None;
        for cid in &candidate_ids {
            let key = normalize_identifier(&cid.scheme, &cid.value);
            if let Some(canonical_id) = self.alias_index.get(&key)
                && self.entities.contains_key(canonical_id)
            {
                resolved_canonical_id = Some(canonical_id.clone());
                break;
            }
        }

        if let Some(canonical_id) = resolved_canonical_id {
            if let Some((entity, entry_seq)) = self.entities.get_mut(&canonical_id) {
                self.seq = self.seq.wrapping_add(1);
                *entry_seq = self.seq;
                let current_seq = self.seq;
                self.lru_order.push_back((canonical_id.clone(), current_seq));

                entity.last_seen = entity.last_seen.max(now_ts);
                entity.confidence = 1.0;

                for cid in candidate_ids {
                    let key = normalize_identifier(&cid.scheme, &cid.value);
                    if !entity.identifiers.iter().any(|existing| {
                        normalize_identifier(&existing.scheme, &existing.value) == key
                    }) {
                        entity.identifiers.push(cid.clone());
                    }
                    self.alias_index.insert(key, canonical_id.clone());
                }

                if let Some(tc) = &ac.type_code {
                    entity.attributes.insert("typeCode".to_string(), tc.clone());
                }
                if let Some(cat) = &ac.category {
                    entity.attributes.insert("category".to_string(), cat.clone());
                }
                if let Some(sq) = &ac.squawk {
                    entity.attributes.insert("squawk".to_string(), sq.clone());
                }
                entity.attributes.insert("isMilitary".to_string(), ac.is_military.to_string());
                entity.attributes.insert("source".to_string(), ac.source.clone());

                if entity.primary_label == hex {
                    if let Some(cs) = &cs_id {
                        entity.primary_label.clone_from(&cs.value);
                    } else if let Some(reg) = &reg_id {
                        entity.primary_label.clone_from(&reg.value);
                    }
                }
            }
        } else {
            let canonical_id = format!("canonical:aircraft:{hex}");
            let primary_label = cs_id
                .as_ref()
                .map(|c| c.value.clone())
                .or_else(|| reg_id.as_ref().map(|r| r.value.clone()))
                .unwrap_or_else(|| hex.clone());

            let mut attributes = HashMap::new();
            if let Some(tc) = &ac.type_code {
                attributes.insert("typeCode".to_string(), tc.clone());
            }
            if let Some(cat) = &ac.category {
                attributes.insert("category".to_string(), cat.clone());
            }
            if let Some(sq) = &ac.squawk {
                attributes.insert("squawk".to_string(), sq.clone());
            }
            attributes.insert("isMilitary".to_string(), ac.is_military.to_string());
            attributes.insert("source".to_string(), ac.source.clone());

            let new_entity = CanonicalEntity {
                id: canonical_id.clone(),
                kind: EntityKind::Aircraft,
                primary_label,
                identifiers: candidate_ids.clone(),
                attributes,
                first_seen: now_ts,
                last_seen: now_ts,
                confidence: 1.0,
            };

            for cid in candidate_ids {
                let key = normalize_identifier(&cid.scheme, &cid.value);
                self.alias_index.insert(key, canonical_id.clone());
                self.links.push_back(EntityLink {
                    source_id: canonical_id.clone(),
                    target_id: format!("{}:{}", cid.scheme, cid.value),
                    relation: "alias_of".to_string(),
                    confidence: 1.0,
                    created_at: now_ts,
                });
            }

            self.insert_entity(new_entity);
        }
    }

    fn ingest_vessel(&mut self, v: &Vessel, now_ts: i64) {
        let mmsi_trimmed = v.mmsi.trim().to_string();
        let mmsi_id = EntityIdentifier::new("mmsi", mmsi_trimmed.clone());
        let imo_id = v.imo.as_ref().and_then(|imo| {
            let trimmed = imo.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(EntityIdentifier::new("imo", trimmed.to_string()))
            }
        });
        let name_id = v.name.as_ref().and_then(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(EntityIdentifier::new("name", trimmed.to_ascii_uppercase()))
            }
        });

        let mut candidate_ids = vec![mmsi_id.clone()];
        if let Some(imo) = &imo_id {
            candidate_ids.push(imo.clone());
        }
        if let Some(name) = &name_id {
            candidate_ids.push(name.clone());
        }

        let mut resolved_canonical_id = None;
        for cid in &candidate_ids {
            let key = normalize_identifier(&cid.scheme, &cid.value);
            if let Some(canonical_id) = self.alias_index.get(&key)
                && self.entities.contains_key(canonical_id)
            {
                resolved_canonical_id = Some(canonical_id.clone());
                break;
            }
        }

        let seen_epoch = v.last_position_epoch.unwrap_or(now_ts);

        if let Some(canonical_id) = resolved_canonical_id {
            if let Some((entity, entry_seq)) = self.entities.get_mut(&canonical_id) {
                self.seq = self.seq.wrapping_add(1);
                *entry_seq = self.seq;
                let current_seq = self.seq;
                self.lru_order.push_back((canonical_id.clone(), current_seq));

                entity.last_seen = entity.last_seen.max(seen_epoch);
                entity.confidence = 1.0;

                for cid in candidate_ids {
                    let key = normalize_identifier(&cid.scheme, &cid.value);
                    if !entity.identifiers.iter().any(|existing| {
                        normalize_identifier(&existing.scheme, &existing.value) == key
                    }) {
                        entity.identifiers.push(cid.clone());
                    }
                    self.alias_index.insert(key, canonical_id.clone());
                }

                if let Some(vt) = &v.vessel_type {
                    entity.attributes.insert("vesselType".to_string(), vt.clone());
                }
                if let Some(dest) = &v.destination {
                    entity.attributes.insert("destination".to_string(), dest.clone());
                }

                if entity.primary_label == mmsi_trimmed
                    && let Some(name) = &name_id
                {
                    entity.primary_label.clone_from(&name.value);
                }
            }
        } else {
            let canonical_id = format!("canonical:vessel:{mmsi_trimmed}");
            let primary_label = v.name.clone().unwrap_or_else(|| mmsi_trimmed.clone());

            let mut attributes = HashMap::new();
            if let Some(vt) = &v.vessel_type {
                attributes.insert("vesselType".to_string(), vt.clone());
            }
            if let Some(dest) = &v.destination {
                attributes.insert("destination".to_string(), dest.clone());
            }

            let new_entity = CanonicalEntity {
                id: canonical_id.clone(),
                kind: EntityKind::Vessel,
                primary_label,
                identifiers: candidate_ids.clone(),
                attributes,
                first_seen: seen_epoch,
                last_seen: seen_epoch,
                confidence: 1.0,
            };

            for cid in candidate_ids {
                let key = normalize_identifier(&cid.scheme, &cid.value);
                self.alias_index.insert(key, canonical_id.clone());
                self.links.push_back(EntityLink {
                    source_id: canonical_id.clone(),
                    target_id: format!("{}:{}", cid.scheme, cid.value),
                    relation: "alias_of".to_string(),
                    confidence: 1.0,
                    created_at: now_ts,
                });
            }

            self.insert_entity(new_entity);
        }
    }

    fn ingest_satellite(&mut self, s: &SatelliteElements, now_ts: i64) {
        let norad_str = s.norad_id.to_string();
        let cat_id = EntityIdentifier::new("norad_cat_id", norad_str.clone());
        let norad_id = EntityIdentifier::new("norad_id", norad_str.clone());
        let intl_id = s.intl_designator.as_ref().and_then(|intl| {
            let trimmed = intl.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(EntityIdentifier::new("intl_designator", trimmed.to_string()))
            }
        });
        let name_id = EntityIdentifier::new("name", s.name.trim().to_ascii_uppercase());

        let mut candidate_ids = vec![cat_id.clone(), norad_id.clone(), name_id.clone()];
        if let Some(intl) = &intl_id {
            candidate_ids.push(intl.clone());
        }

        let mut resolved_canonical_id = None;
        for cid in &candidate_ids {
            let key = normalize_identifier(&cid.scheme, &cid.value);
            if let Some(canonical_id) = self.alias_index.get(&key)
                && self.entities.contains_key(canonical_id)
            {
                resolved_canonical_id = Some(canonical_id.clone());
                break;
            }
        }

        let seen_epoch = s.epoch.as_second();

        if let Some(canonical_id) = resolved_canonical_id {
            if let Some((entity, entry_seq)) = self.entities.get_mut(&canonical_id) {
                self.seq = self.seq.wrapping_add(1);
                *entry_seq = self.seq;
                let current_seq = self.seq;
                self.lru_order.push_back((canonical_id.clone(), current_seq));

                entity.last_seen = entity.last_seen.max(seen_epoch);
                entity.confidence = 1.0;

                for cid in candidate_ids {
                    let key = normalize_identifier(&cid.scheme, &cid.value);
                    if !entity.identifiers.iter().any(|existing| {
                        normalize_identifier(&existing.scheme, &existing.value) == key
                    }) {
                        entity.identifiers.push(cid.clone());
                    }
                    self.alias_index.insert(key, canonical_id.clone());
                }

                entity.attributes.insert("group".to_string(), s.group.clone());
                if let Some(cls) = &s.classification {
                    entity.attributes.insert("classification".to_string(), cls.clone());
                }
            }
        } else {
            let canonical_id = format!("canonical:satellite:{norad_str}");
            let mut attributes = HashMap::new();
            attributes.insert("group".to_string(), s.group.clone());
            if let Some(cls) = &s.classification {
                attributes.insert("classification".to_string(), cls.clone());
            }

            let new_entity = CanonicalEntity {
                id: canonical_id.clone(),
                kind: EntityKind::Satellite,
                primary_label: s.name.clone(),
                identifiers: candidate_ids.clone(),
                attributes,
                first_seen: seen_epoch,
                last_seen: seen_epoch,
                confidence: 1.0,
            };

            for cid in candidate_ids {
                let key = normalize_identifier(&cid.scheme, &cid.value);
                self.alias_index.insert(key, canonical_id.clone());
                self.links.push_back(EntityLink {
                    source_id: canonical_id.clone(),
                    target_id: format!("{}:{}", cid.scheme, cid.value),
                    relation: "alias_of".to_string(),
                    confidence: 1.0,
                    created_at: now_ts,
                });
            }

            self.insert_entity(new_entity);
        }
    }
}

/// Thread-safe in-memory entity resolution engine with bounded LRU capacity.
#[derive(Debug)]
pub struct ResolutionEngine {
    inner: RwLock<ResolutionInner>,
}

impl ResolutionEngine {
    /// Creates a new resolution engine with the specified bounded capacity.
    #[must_use]
    pub fn new(config: ResolutionConfig) -> Self {
        Self { inner: RwLock::new(ResolutionInner::new(config)) }
    }

    /// Resolves a canonical entity by any known scheme and identifier value.
    ///
    /// Updates the entity's position in the LRU eviction queue.
    #[must_use]
    pub fn resolve(&self, scheme: &str, value: &str) -> Option<CanonicalEntity> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.resolve(scheme, value)
    }

    /// Retrieves a canonical entity by its unique canonical ID.
    ///
    /// Updates the entity's position in the LRU eviction queue.
    #[must_use]
    pub fn get_entity(&self, id: &str) -> Option<CanonicalEntity> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.get_entity(id)
    }

    /// Retrieves all identifiers associated with a canonical entity.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the entity does not exist.
    pub fn get_entity_aliases(
        &self,
        canonical_id: &str,
    ) -> Result<Vec<EntityIdentifier>, CoreError> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.get_entity_aliases(canonical_id)
    }

    /// Retrieves all directed relation links connected to the specified entity ID.
    #[must_use]
    pub fn get_entity_links(&self, id: &str) -> Vec<EntityLink> {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.get_entity_links(id)
    }

    /// Links an alias identifier to a canonical entity, detaching it from any
    /// previous entity and recording provenance links.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the canonical entity does not exist,
    /// or [`CoreError::BadRequest`] if the identifier is invalid.
    pub fn link_alias(
        &self,
        canonical_id: &str,
        identifier: &EntityIdentifier,
    ) -> Result<CanonicalEntity, CoreError> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.link_alias(canonical_id, identifier)
    }

    /// Splits an alias identifier off an existing entity, creating a new standalone
    /// canonical entity for it with provenance.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the entity or identifier is not found,
    /// or [`CoreError::BadRequest`] if attempting to split the entity's sole identifier.
    pub fn split_alias(
        &self,
        canonical_id: &str,
        identifier: &EntityIdentifier,
    ) -> Result<CanonicalEntity, CoreError> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.split_alias(canonical_id, identifier)
    }

    /// Manually merges a source canonical entity into a target canonical entity.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if either entity is missing.
    pub fn merge_entities(
        &self,
        target_id: &str,
        source_id: &str,
    ) -> Result<CanonicalEntity, CoreError> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.merge_entities(target_id, source_id)
    }

    /// Ingests a feed snapshot (aircraft, vessels, satellites), auto-linking and
    /// updating canonical entity records.
    #[must_use]
    pub fn ingest_feed_snapshot(&self, snapshot: &FeedSnapshot) -> usize {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.ingest_feed_snapshot(snapshot)
    }

    /// Number of canonical entities currently in memory.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.entities.len()
    }

    /// Number of alias mapping keys currently in index.
    #[must_use]
    pub fn alias_count(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.alias_index.len()
    }

    /// Number of relation links currently retained.
    #[must_use]
    pub fn link_count(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.links.len()
    }
}
