//! Director-led scene tours and evidence packs (`LB-118`), porting
//! `legacy/src/scenes/nepalEvidencePack.js` and `legacy/src/director/camera.js`.
//!
//! The eleven expansion beats of the Nepal pack carry the exact authored poses from the legacy
//! source. The six base beats come from the legacy source event pack, which is not part of the
//! imported tree, so their poses are approximate framings of the named places.

use lavender_domain::{EvidencePack, SceneCameraPose, SceneShot};

/// Default flight transition for authored shots, in seconds.
pub const DEFAULT_SHOT_DURATION_S: f64 = 4.2;

fn pose(lat: f64, lon: f64, alt_m: f64, heading_deg: f64, pitch_deg: f64) -> SceneCameraPose {
    SceneCameraPose { lat, lon, alt_m, heading_deg, pitch_deg, roll_deg: 0.0 }
}

fn shot(id: &str, title: &str, pose: SceneCameraPose, hold_s: f64) -> SceneShot {
    SceneShot {
        id: id.to_owned(),
        title: title.to_owned(),
        pose,
        duration_s: DEFAULT_SHOT_DURATION_S,
        hold_s,
        map_stack: "photoreal".to_owned(),
        narration: None,
        reach_duration_s: None,
        locator_presentation: None,
    }
}

fn with_reach(mut s: SceneShot, reach_s: f64) -> SceneShot {
    s.reach_duration_s = Some(reach_s);
    s
}

#[allow(clippy::too_many_lines)]
fn nepal_pack() -> EvidencePack {
    let mut final_view = shot(
        "final-view",
        "Final view",
        SceneCameraPose {
            lat: 28.0834,
            lon: 85.206,
            alt_m: 113_520.0,
            heading_deg: 64.0,
            pitch_deg: -83.0,
            roll_deg: 174.0,
        },
        11.0,
    );
    final_view.duration_s = 4.0;
    final_view.locator_presentation = Some("bhote-koshi-incident-places".to_owned());

    let shots = vec![
        shot(
            "immediate-collapse-viewpoint",
            "Immediate collapse viewpoint",
            pose(28.4300, 85.5700, 9_500.0, 220.0, -38.0),
            10.0,
        ),
        with_reach(
            shot(
                "debris-dammed-lake",
                "Debris-dammed lake",
                pose(28.4100, 85.5500, 9_800.0, 222.0, -38.0),
                11.0,
            ),
            4.8,
        ),
        with_reach(
            shot(
                "second-landslide",
                "Second landslide",
                pose(28.399_234, 85.535_806, 10_145.0, 224.0, -38.0),
                11.0,
            ),
            6.5,
        ),
        shot(
            "gyirong-border-gate",
            "Gyirong border gate",
            pose(28.2800, 85.3800, 6_800.0, 205.0, -38.0),
            8.0,
        ),
        with_reach(
            shot(
                "timure-cluster",
                "Timure cluster",
                pose(28.2300, 85.3600, 6_600.0, 207.0, -38.0),
                12.0,
            ),
            2.1,
        ),
        with_reach(
            shot("syabru-besi", "Syabru Besi", pose(28.1650, 85.3400, 6_400.0, 208.0, -39.0), 11.0),
            2.8,
        ),
        with_reach(
            shot("dhunche", "Dhunche", pose(28.161_431, 85.324_278, 6_212.0, 208.0, -39.0), 16.0),
            2.5,
        ),
        with_reach(
            shot(
                "mailung-upper-trishuli",
                "Mailung / Upper Trishuli bridge",
                pose(28.140_642, 85.265_996, 6_353.0, 211.0, -40.0),
                13.0,
            ),
            2.9,
        ),
        with_reach(
            shot(
                "mailung-bazzar",
                "Mailung Bazzar, Dandaguan",
                pose(28.126_526, 85.248_027, 6_331.0, 212.0, -40.0),
                15.0,
            ),
            2.1,
        ),
        with_reach(
            shot(
                "dandagaun",
                "Dandagaun, Rasuwa",
                pose(28.124_424, 85.245_154, 6_311.0, 214.0, -40.0),
                15.0,
            ),
            1.2,
        ),
        with_reach(
            shot(
                "dandagaun-viewpoint",
                "Dandagaun viewpoint restaurant",
                pose(28.086_663, 85.229_914, 6_740.0, 215.0, -41.0),
                13.0,
            ),
            2.5,
        ),
        with_reach(
            shot(
                "betrawati-bazaar",
                "Betrawati Bazaar",
                pose(28.029_827, 85.227_694, 7_034.0, 216.0, -41.0),
                12.0,
            ),
            2.9,
        ),
        with_reach(
            shot("bhainse", "Bhainse", pose(28.023_087, 85.229_449, 8_051.0, 219.0, -41.0), 13.0),
            1.5,
        ),
        with_reach(
            shot(
                "bidur-trishuli-bridge",
                "Bidur / Trishuli bridge",
                pose(27.9000, 85.1800, 8_800.0, 222.0, -42.0),
                12.0,
            ),
            2.6,
        ),
        shot(
            "devighat-taadi-khola-bridge",
            "Devighat / Taadi Khola Bridge",
            pose(27.921_132, 85.177_113, 9_056.0, 224.0, -42.0),
            13.0,
        ),
        shot("charaudi", "Charaudi", pose(27.865_095, 84.838_424, 10_347.0, 232.0, -42.0), 11.0),
        final_view,
    ];

    EvidencePack {
        id: "bhote-koshi-nepal".to_owned(),
        title: "Bhote Koshi / Rasuwa landslide and flood".to_owned(),
        summary: "Seventeen-beat evidence tour following the debris flow down the Bhote Koshi and Trishuli valleys.".to_owned(),
        region: "Nepal / Tibet border".to_owned(),
        required_layers: vec!["satellites".to_owned(), "fires".to_owned()],
        shots,
    }
}

fn maritime_pack() -> EvidencePack {
    EvidencePack {
        id: "hormuz-bab-el-mandeb-maritime".to_owned(),
        title: "Strategic maritime chokepoints".to_owned(),
        summary: "Suez, Bab el-Mandeb, Hormuz and Malacca traffic corridors.".to_owned(),
        region: "Red Sea / Persian Gulf / Strait of Malacca".to_owned(),
        required_layers: vec!["vessels".to_owned()],
        shots: vec![
            shot("suez", "Suez Canal", pose(30.4500, 32.3500, 60_000.0, 0.0, -60.0), 8.0),
            shot(
                "bab-el-mandeb",
                "Bab el-Mandeb",
                pose(12.6000, 43.3000, 90_000.0, 330.0, -55.0),
                8.0,
            ),
            shot("hormuz", "Strait of Hormuz", pose(26.5700, 56.2500, 110_000.0, 40.0, -55.0), 8.0),
            shot("malacca", "Strait of Malacca", pose(2.5000, 101.5, 150_000.0, 300.0, -55.0), 8.0),
        ],
    }
}

fn spaceport_pack() -> EvidencePack {
    EvidencePack {
        id: "global-spaceport-corridors".to_owned(),
        title: "Global launch corridors".to_owned(),
        summary: "Major orbital launch sites and their downrange safety corridors.".to_owned(),
        region: "Global".to_owned(),
        required_layers: vec!["launches".to_owned()],
        shots: vec![
            shot(
                "cape-canaveral-lc39a",
                "Cape Canaveral LC-39A",
                pose(28.6080, -80.6040, 20_000.0, 90.0, -50.0),
                8.0,
            ),
            shot(
                "starbase",
                "Starbase, Boca Chica",
                pose(25.9970, -97.1560, 15_000.0, 70.0, -50.0),
                8.0,
            ),
            shot(
                "vandenberg-slc4e",
                "Vandenberg SLC-4E",
                pose(34.6320, -120.6110, 20_000.0, 200.0, -50.0),
                8.0,
            ),
            shot("kourou", "Kourou", pose(5.2390, -52.7680, 20_000.0, 60.0, -50.0), 8.0),
            shot("mahia", "Mahia Peninsula", pose(-39.2600, 177.8650, 20_000.0, 20.0, -50.0), 8.0),
        ],
    }
}

/// Returns the curated evidence packs.
#[must_use]
pub fn evidence_pack_catalog() -> Vec<EvidencePack> {
    vec![nepal_pack(), maritime_pack(), spaceport_pack()]
}

fn wrap_longitude(lon: f64) -> f64 {
    ((lon + 180.0).rem_euclid(360.0)) - 180.0
}

fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let delta = (b - a + 540.0).rem_euclid(360.0) - 180.0;
    a + delta * t
}

/// Samples an authored camera move; mirrors `sampleCameraMove` in `legacy/src/director/camera.js`.
///
/// Cubic ease-in-out unless `linear`; heading, roll and longitude take the short arc.
#[must_use]
pub fn sample_camera_move(
    from: &SceneCameraPose,
    to: &SceneCameraPose,
    progress: f64,
    linear: bool,
) -> SceneCameraPose {
    let t = if progress.is_finite() { progress.clamp(0.0, 1.0) } else { 0.0 };
    if t <= 0.0 {
        return *from;
    }
    if t >= 1.0 {
        return *to;
    }
    let eased = if linear {
        t
    } else if t < 0.5 {
        4.0 * t.powi(3)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    };
    let lerp = |a: f64, b: f64| a + (b - a) * eased;
    SceneCameraPose {
        lat: lerp(from.lat, to.lat),
        lon: wrap_longitude(lerp_angle(from.lon, to.lon, eased)),
        alt_m: lerp(from.alt_m, to.alt_m),
        heading_deg: lerp_angle(from.heading_deg, to.heading_deg, eased),
        pitch_deg: lerp(from.pitch_deg, to.pitch_deg),
        roll_deg: lerp_angle(from.roll_deg, to.roll_deg, eased),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nepal_pack_has_the_seventeen_legacy_beats_in_order() {
        let pack = nepal_pack();
        let ids: Vec<&str> = pack.shots.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "immediate-collapse-viewpoint",
                "debris-dammed-lake",
                "second-landslide",
                "gyirong-border-gate",
                "timure-cluster",
                "syabru-besi",
                "dhunche",
                "mailung-upper-trishuli",
                "mailung-bazzar",
                "dandagaun",
                "dandagaun-viewpoint",
                "betrawati-bazaar",
                "bhainse",
                "bidur-trishuli-bridge",
                "devighat-taadi-khola-bridge",
                "charaudi",
                "final-view",
            ]
        );
        let last = pack.shots.last().map(|s| (s.pose.roll_deg, s.duration_s));
        assert_eq!(last, Some((174.0, 4.0)));
    }

    #[test]
    fn catalog_lists_three_packs() {
        let ids: Vec<String> = evidence_pack_catalog().into_iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn camera_move_endpoints_and_cubic_midpoint() {
        let a = pose(0.0, 0.0, 100.0, 0.0, -30.0);
        let b = pose(10.0, 10.0, 300.0, 90.0, -60.0);
        assert_eq!(sample_camera_move(&a, &b, 0.0, false), a);
        assert_eq!(sample_camera_move(&a, &b, 1.0, false), b);
        let mid = sample_camera_move(&a, &b, 0.5, false);
        assert!((mid.lat - 5.0).abs() < 1e-9);
        let quarter = sample_camera_move(&a, &b, 0.25, false);
        assert!((quarter.lat - 10.0 * 4.0 * 0.25_f64.powi(3)).abs() < 1e-9);
        let linear = sample_camera_move(&a, &b, 0.25, true);
        assert!((linear.lat - 2.5).abs() < 1e-9);
    }

    #[test]
    fn longitude_takes_the_short_arc_across_the_dateline() {
        let a = pose(0.0, 170.0, 0.0, 350.0, 0.0);
        let b = pose(0.0, -170.0, 0.0, 10.0, 0.0);
        let mid = sample_camera_move(&a, &b, 0.5, true);
        assert!((mid.lon.abs() - 180.0).abs() < 1e-9);
        assert!(mid.heading_deg.abs() < 1e-9 || (mid.heading_deg - 360.0).abs() < 1e-9);
    }
}
