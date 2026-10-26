//! Pure Awareness (Contacts) model (`LB-123`), porting `legacy/src/layers/awareness/model.js`
//! and the policy constants it consumes from `policy.js`. Nothing here performs I/O or holds
//! state; every input is explicit and every output is bounded.

use lavender_domain::{
    AwarenessContextSnapshot, AwarenessNavigation, AwarenessRefreshDecision, AwarenessResults,
    ContactsWindow,
};

/// Parked-camera refresh cadence for Contacts readouts, in milliseconds.
pub const AWARENESS_REFRESH_MS: i64 = 750;
/// Refresh cadence while the camera pose is changing, in milliseconds.
pub const AWARENESS_MOTION_REFRESH_MS: i64 = 175;
/// How long the pose must stay unchanged before the view counts as parked, in milliseconds.
pub const AWARENESS_MOTION_SETTLE_MS: i64 = 250;
/// Subject displacement that forces a cohort rescan, in metres.
pub const AWARENESS_REEVALUATE_DISTANCE_M: f64 = 250.0;
/// Hard cap on nearest-contact identifiers retained per cohort in a snapshot.
pub const AWARENESS_MAX_NAVIGATION_EXAMPLES: usize = 10_000;

/// Whether focusing a Context target may take ownership of the camera. Cockpit mode owns the
/// camera, so vessel and installation targets stay selectable but must not start a flight.
#[must_use]
pub fn context_target_fly_to_allowed(layer_id: &str, cockpit_mode: bool) -> bool {
    let non_aircraft = matches!(layer_id, "ais-live-vessels" | "military-installations");
    !(non_aircraft && cockpit_mode)
}

/// Minimum milliseconds between readout refreshes for the given camera state.
#[must_use]
pub const fn refresh_interval_ms(camera_moving: bool) -> i64 {
    if camera_moving { AWARENESS_MOTION_REFRESH_MS } else { AWARENESS_REFRESH_MS }
}

/// Decides, for one frame, whether the view counts as moving and whether the readout may
/// refresh. Motion is hysteretic: it ends only after the pose has been unchanged for
/// [`AWARENESS_MOTION_SETTLE_MS`], and the motion cadence is a hard floor in every state.
#[must_use]
pub fn refresh_decision(
    now_ms: i64,
    last_refresh_ms: i64,
    last_pose_change_ms: i64,
    was_moving: bool,
) -> AwarenessRefreshDecision {
    let moving = now_ms.saturating_sub(last_pose_change_ms) < AWARENESS_MOTION_SETTLE_MS;
    let settling = was_moving && !moving;
    let since_refresh = now_ms.saturating_sub(last_refresh_ms);
    let refresh = since_refresh >= refresh_interval_ms(true)
        && (moving || settling || since_refresh >= refresh_interval_ms(false));
    AwarenessRefreshDecision { moving, refresh }
}

/// Whether a source-scoped clear was an eviction (the contact aged out) rather than a deselect.
#[must_use]
pub fn clear_is_eviction(reason: Option<&str>) -> bool {
    reason == Some("evicted")
}

/// Whether proximity cohorts need an expensive rescan. A non-finite or missing displacement
/// always rescans.
#[must_use]
pub fn refresh_required(
    force: bool,
    has_results: bool,
    movement_m: f64,
    source_revision_changed: bool,
) -> bool {
    force
        || !has_results
        || source_revision_changed
        || !movement_m.is_finite()
        || movement_m >= AWARENESS_REEVALUATE_DISTANCE_M
}

/// Navigation affordances from the history index, subject presence and forward availability.
#[must_use]
pub const fn navigation_state(
    navigation_index: usize,
    has_subject: bool,
    can_navigate_next: bool,
) -> AwarenessNavigation {
    AwarenessNavigation {
        can_previous: navigation_index > 0,
        can_focus: has_subject,
        can_next: can_navigate_next,
    }
}

/// Whether Contacts has per-frame work: only a moving camera with a subject and live results.
#[must_use]
pub const fn needs_continuous_render(
    camera_moving: bool,
    has_subject: bool,
    has_live_results: bool,
) -> bool {
    camera_moving && has_subject && has_live_results
}

/// Builds the read-only snapshot shared with compact HUD consumers. Nearest lists are capped
/// at [`AWARENESS_MAX_NAVIGATION_EXAMPLES`] and flagged truncated when cut.
#[must_use]
pub fn build_context_snapshot(
    results: &AwarenessResults,
    navigation: AwarenessNavigation,
    subject_present: bool,
) -> AwarenessContextSnapshot {
    let mut cohorts = results.cohorts.clone();
    for cohort in &mut cohorts {
        if cohort.nearest.len() > AWARENESS_MAX_NAVIGATION_EXAMPLES {
            cohort.nearest.truncate(AWARENESS_MAX_NAVIGATION_EXAMPLES);
            cohort.truncated = true;
        }
    }
    AwarenessContextSnapshot {
        subject: results.subject.clone(),
        subject_present,
        evaluated_at: results.evaluated_at,
        radius_m: results.radius_m,
        cohorts,
        navigation,
    }
}

/// Derives the panel-equivalent counts from a snapshot, so the panel and any tool answer
/// cannot drift. A cohort that cannot answer reports `None` (unknown), never a misleading zero.
#[must_use]
pub fn contacts_window(snapshot: &AwarenessContextSnapshot) -> ContactsWindow {
    let find = |id: &str| snapshot.cohorts.iter().find(|c| c.id == id);
    let count_for = |id: &str| find(id).and_then(|c| c.count);
    let flights = count_for("flights");
    let military = count_for("military");
    let aircraft = match (flights, military) {
        (Some(f), Some(m)) => Some(f.saturating_add(m)),
        _ => None,
    };
    let complete = ["flights", "military"]
        .iter()
        .all(|id| find(id).is_some_and(|c| c.count.is_some() && c.complete));
    let radius_km = snapshot.radius_m.filter(|r| r.is_finite() && *r >= 0.0).map(|r| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let km = (r / 1000.0).round().min(f64::from(u32::MAX)) as u32;
        km
    });
    ContactsWindow {
        centered_on: snapshot
            .subject
            .label
            .clone()
            .filter(|l| !l.is_empty())
            .or_else(|| Some(snapshot.subject.id.clone()).filter(|i| !i.is_empty())),
        radius_km,
        aircraft,
        flights,
        military,
        vessels: count_for("ais-live-vessels"),
        complete,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use lavender_domain::{AwarenessCohort, AwarenessSubject};

    use super::*;

    fn cohort(id: &str, count: Option<u32>, complete: bool) -> AwarenessCohort {
        AwarenessCohort {
            id: id.to_owned(),
            label: id.to_owned(),
            source: "test".to_owned(),
            coverage: None,
            relationship: "within range".to_owned(),
            count,
            complete,
            truncated: false,
            reason: None,
            nearest: vec![],
        }
    }

    fn results(cohorts: Vec<AwarenessCohort>) -> AwarenessResults {
        AwarenessResults {
            subject: AwarenessSubject { id: "abc123".into(), label: Some("UAL1".into()) },
            evaluated_at: jiff::Timestamp::UNIX_EPOCH,
            radius_m: Some(250_400.0),
            cohorts,
        }
    }

    #[test]
    fn fly_to_blocked_only_for_non_aircraft_in_cockpit() {
        assert!(!context_target_fly_to_allowed("ais-live-vessels", true));
        assert!(!context_target_fly_to_allowed("military-installations", true));
        assert!(context_target_fly_to_allowed("ais-live-vessels", false));
        assert!(context_target_fly_to_allowed("flights", true));
    }

    #[test]
    fn refresh_decision_is_hysteretic_with_motion_floor() {
        assert_eq!(refresh_interval_ms(true), 175);
        assert_eq!(refresh_interval_ms(false), 750);
        // Moving, but under the 175 ms floor: no refresh.
        let d = refresh_decision(1_100, 1_000, 1_090, true);
        assert!(d.moving && !d.refresh);
        // Moving past the floor: refresh.
        let d = refresh_decision(1_200, 1_000, 1_190, true);
        assert!(d.moving && d.refresh);
        // Settle frame: motion just expired, refresh once.
        let d = refresh_decision(1_400, 1_000, 1_100, true);
        assert!(!d.moving && d.refresh);
        // Parked, short of the parked cadence: quiet.
        let d = refresh_decision(1_400, 1_000, 1_100, false);
        assert!(!d.moving && !d.refresh);
        // Parked past the cadence: refresh.
        let d = refresh_decision(2_000, 1_000, 100, false);
        assert!(!d.moving && d.refresh);
        // Extreme clocks never overflow.
        let _ = refresh_decision(i64::MAX, i64::MIN, i64::MIN, true);
    }

    #[test]
    fn eviction_and_refresh_required() {
        assert!(clear_is_eviction(Some("evicted")));
        assert!(!clear_is_eviction(Some("deselected")));
        assert!(!clear_is_eviction(None));
        assert!(!refresh_required(false, true, 10.0, false));
        assert!(refresh_required(true, true, 10.0, false));
        assert!(refresh_required(false, false, 10.0, false));
        assert!(refresh_required(false, true, 10.0, true));
        assert!(refresh_required(false, true, 250.0, false));
        assert!(refresh_required(false, true, f64::NAN, false));
    }

    #[test]
    fn navigation_and_render_gating() {
        let n = navigation_state(0, true, false);
        assert!(!n.can_previous && n.can_focus && !n.can_next);
        assert!(navigation_state(2, false, true).can_previous);
        assert!(needs_continuous_render(true, true, true));
        assert!(!needs_continuous_render(false, true, true));
        assert!(!needs_continuous_render(true, false, true));
        assert!(!needs_continuous_render(true, true, false));
    }

    #[test]
    fn snapshot_caps_nearest_and_marks_truncation() {
        let mut big = cohort("flights", Some(1), true);
        big.nearest = (0..=AWARENESS_MAX_NAVIGATION_EXAMPLES).map(|i| i.to_string()).collect();
        let snap = build_context_snapshot(
            &results(vec![big, cohort("military", Some(2), true)]),
            AwarenessNavigation::default(),
            false,
        );
        assert!(!snap.subject_present);
        assert_eq!(snap.cohorts[0].nearest.len(), AWARENESS_MAX_NAVIGATION_EXAMPLES);
        assert!(snap.cohorts[0].truncated);
        assert!(!snap.cohorts[1].truncated);
    }

    #[test]
    fn contacts_window_counts_and_unknowns() {
        let snap = build_context_snapshot(
            &results(vec![
                cohort("flights", Some(30), true),
                cohort("military", Some(12), true),
                cohort("ais-live-vessels", None, false),
            ]),
            AwarenessNavigation::default(),
            true,
        );
        let w = contacts_window(&snap);
        assert_eq!(w.centered_on.as_deref(), Some("UAL1"));
        assert_eq!(w.radius_km, Some(250));
        assert_eq!((w.flights, w.military, w.aircraft), (Some(30), Some(12), Some(42)));
        assert_eq!(w.vessels, None);
        assert!(w.complete);

        let partial = build_context_snapshot(
            &results(vec![cohort("flights", Some(30), false), cohort("military", None, true)]),
            AwarenessNavigation::default(),
            true,
        );
        let w = contacts_window(&partial);
        assert_eq!(w.aircraft, None);
        assert!(!w.complete);
        let missing =
            build_context_snapshot(&results(vec![]), AwarenessNavigation::default(), true);
        assert!(!contacts_window(&missing).complete);
    }
}
