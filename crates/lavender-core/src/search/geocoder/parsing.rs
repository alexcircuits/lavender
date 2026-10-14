//! Wire response parsing is deliberately independent from fetching and caching.

use std::collections::HashSet;
use std::time::Instant;

use lavender_domain::text::clean;
use lavender_domain::{BBox, GeoPoint, PlaceHit};
use serde_json::Value;

use crate::config::GeocoderFormat;

use super::{GeocodedPlaces, GeocoderError};

fn text(value: &Value) -> Option<String> {
    let text = clean(value.as_str()?, 200);
    (!text.is_empty()).then_some(text)
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok()).filter(|n| n.is_finite())
}

fn context(properties: &Value, name: &str) -> Option<String> {
    let mut seen = HashSet::from([name.to_lowercase()]);
    let parts: Vec<_> = ["city", "county", "state", "country"]
        .iter()
        .filter_map(|key| text(&properties[key]))
        .filter(|part| seen.insert(part.to_lowercase()))
        .collect();
    (!parts.is_empty()).then(|| clean(&parts.join(", "), 200))
}

fn bbox(values: &Value, format: GeocoderFormat) -> Option<BBox> {
    let values = values.as_array()?;
    if values.len() != 4 {
        return None;
    }
    let a = number(&values[0])?;
    let b = number(&values[1])?;
    let c = number(&values[2])?;
    let d = number(&values[3])?;
    match format {
        GeocoderFormat::Nominatim => BBox::new(c, a, d, b).ok(),
        // Photon extent is minLon,maxLat,maxLon,minLat, unlike GeoJSON bbox.
        GeocoderFormat::Photon => BBox::new(a, d, c, b).ok(),
    }
}

fn nominatim(row: &Value, source_id: &str) -> Option<PlaceHit> {
    let display = text(&row["display_name"]);
    let name = text(&row["name"]).or_else(|| {
        let name = clean(display.as_ref()?.split(',').next()?, 200);
        (!name.is_empty()).then_some(name)
    })?;
    let position = GeoPoint::new(number(&row["lat"])?, number(&row["lon"])?).ok()?;
    let context = context(&row["address"], &name).or_else(|| {
        let (_, rest) = display.as_ref()?.split_once(',')?;
        let rest = clean(rest, 200);
        (!rest.is_empty()).then_some(rest)
    });
    Some(PlaceHit {
        name,
        context,
        position,
        bbox: bbox(&row["boundingbox"], GeocoderFormat::Nominatim),
        kind: text(&row["addresstype"]).or_else(|| text(&row["type"])),
        source_id: source_id.to_owned(),
    })
}

fn photon(row: &Value, source_id: &str) -> Option<PlaceHit> {
    if row["geometry"]["type"].as_str()? != "Point" {
        return None;
    }
    let coordinates = row["geometry"]["coordinates"].as_array()?;
    if coordinates.len() < 2 {
        return None;
    }
    let properties = &row["properties"];
    let name = text(&properties["name"]).or_else(|| text(&properties["street"]))?;
    let position = GeoPoint::new(number(&coordinates[1])?, number(&coordinates[0])?).ok()?;
    Some(PlaceHit {
        context: context(properties, &name),
        name,
        position,
        bbox: bbox(&properties["extent"], GeocoderFormat::Photon),
        kind: text(&properties["osm_value"]).or_else(|| text(&properties["type"])),
        source_id: source_id.to_owned(),
    })
}

pub(super) fn parse(
    body: &[u8],
    format: GeocoderFormat,
    source_id: &str,
    limit: usize,
) -> Result<GeocodedPlaces, GeocoderError> {
    // serde error strings may quote a search or a malicious upstream value.
    let document: Value = serde_json::from_slice(body).map_err(|_| GeocoderError::Malformed)?;
    let rows = match format {
        GeocoderFormat::Nominatim => document.as_array(),
        GeocoderFormat::Photon => document["features"].as_array(),
    }
    .ok_or(GeocoderError::Malformed)?;
    let mut places = Vec::new();
    let mut dropped = 0;
    let mut seen = HashSet::new();
    for row in rows {
        let hit = match format {
            GeocoderFormat::Nominatim => nominatim(row, source_id),
            GeocoderFormat::Photon => photon(row, source_id),
        };
        if let Some(hit) = hit {
            // Keep the upstream's relevance order while collapsing duplicate rows.
            let key =
                (hit.name.to_lowercase(), hit.position.lat.to_bits(), hit.position.lon.to_bits());
            if seen.insert(key) && places.len() < limit {
                places.push(hit);
            }
        } else {
            dropped += 1;
        }
    }
    if !rows.is_empty() && dropped == rows.len() {
        return Err(GeocoderError::Malformed);
    }
    let notices = if dropped == 0 {
        Vec::new()
    } else {
        vec![format!("Place search: {dropped} malformed records skipped.")]
    };
    Ok(GeocodedPlaces { places, notices, fetched_at: Instant::now() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_distinguishes_empty_documents_from_invalid_shapes() {
        for (format, empty) in
            [(GeocoderFormat::Nominatim, "[]"), (GeocoderFormat::Photon, r#"{"features":[]}"#)]
        {
            assert_eq!(
                parse(empty.as_bytes(), format, "source", 8).unwrap().places,
                Vec::<PlaceHit>::new()
            );
            for bad in ["null", "<html>secret</html>", r#"{"features":{}}"#] {
                assert_eq!(
                    parse(bad.as_bytes(), format, "source", 8).unwrap_err(),
                    GeocoderError::Malformed
                );
            }
        }
    }

    #[test]
    fn normalization_bounds_text_and_keeps_antimeridian_extents() {
        let response = serde_json::json!({"features": [
            {"geometry":{"type":"Point","coordinates":[179.5,1.0]},"properties":{
                "name": format!("\u{0}{}", "名".repeat(500)),
                "country": "  Pacific\nIslands ",
                "extent": [170.0,10.0,-170.0,-10.0]
            }},
            {"geometry":{"type":"Point","coordinates":[0.0]},"properties":{"name":"Bad"}},
            {"geometry":{"type":"Polygon","coordinates":[]},"properties":{"name":"Bad"}}
        ]});
        let result =
            parse(&serde_json::to_vec(&response).unwrap(), GeocoderFormat::Photon, "source", 8)
                .unwrap();
        assert_eq!(result.places.len(), 1);
        let place = &result.places[0];
        assert_eq!(place.name.chars().count(), 200);
        assert!(!place.name.chars().any(char::is_control));
        assert_eq!(place.context.as_deref(), Some("Pacific Islands"));
        assert!(place.bbox.unwrap().crosses_antimeridian());
        assert_eq!(result.notices, vec!["Place search: 2 malformed records skipped."]);
    }
}
