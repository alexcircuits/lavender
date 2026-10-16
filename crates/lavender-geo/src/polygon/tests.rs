use super::*;
use proptest::prelude::*;

/// A closed square with corners at the given longitude/latitude offsets.
fn square(lon: f64, lat: f64, size: f64) -> Vec<Position> {
    vec![[lon, lat], [lon + size, lat], [lon + size, lat + size], [lon, lat + size], [lon, lat]]
}

/// Indices of `simplified` inside `original`, proving it is an in-order subsequence.
#[allow(clippy::float_cmp)] // Simplification copies positions; they must be identical.
fn kept_indices(original: &[Position], simplified: &[Position]) -> Option<Vec<usize>> {
    let mut indices = Vec::with_capacity(simplified.len());
    let mut from = 0;
    for position in simplified {
        let found = (from..original.len()).find(|&i| original[i] == *position)?;
        indices.push(found);
        from = found + 1;
    }
    Some(indices)
}

#[test]
fn area_sign_follows_winding() {
    let ccw = square(-120.0, 45.0, 0.01);
    assert!(signed_area(&ccw) > 0.0);
    let cw: Vec<Position> = ccw.iter().rev().copied().collect();
    assert!((signed_area(&cw) + signed_area(&ccw)).abs() < 1e-18);
    assert!((signed_area(&ccw) - 1e-4).abs() < 1e-12);
}

#[test]
fn cleaning_drops_repeats_and_orients_exteriors_and_holes() {
    let mut ring = square(-110.0, 40.0, 0.1);
    ring.insert(1, ring[0]);
    ring.insert(3, ring[2]);
    let exterior = clean_ring(&ring, true).unwrap();
    assert_eq!(exterior.len(), 5);
    assert!(signed_area(&exterior) > 0.0);
    let hole = clean_ring(&ring, false).unwrap();
    assert!(signed_area(&hole) < 0.0);
    assert_eq!(hole.first(), hole.last());
}

#[test]
fn cleaning_rejects_open_tiny_and_flat_rings() {
    let mut open = square(-110.0, 40.0, 0.1);
    open.pop();
    assert_eq!(clean_ring(&open, true), None);
    let flat = vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [0.0, 0.0]];
    assert_eq!(clean_ring(&flat, true), None);
    let repeated = vec![[5.0, 5.0], [6.0, 5.0], [6.0, 5.0], [5.0, 5.0]];
    assert_eq!(clean_ring(&repeated, true), None);
    assert_eq!(clean_ring(&[], true), None);
}

#[test]
fn a_tiny_fire_keeps_a_triangle_at_any_tolerance() {
    let ring = vec![[-118.0, 34.0], [-117.9999, 34.0], [-117.99995, 34.0001], [-118.0, 34.0]];
    let simplified = simplify_ring(&ring, 50_000.0).unwrap();
    assert_eq!(simplified, ring);
    let large = square(-118.0, 34.0, 0.001);
    assert_eq!(simplify_ring(&large, 1e9).unwrap().len(), 4);
}

#[test]
fn collinear_and_open_rings_do_not_simplify() {
    let flat = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [0.0, 0.0]];
    assert_eq!(simplify_ring(&flat, 1.0), None);
    let point = vec![[3.0, 3.0]; 4];
    assert_eq!(simplify_ring(&point, 1.0), None);
    assert_eq!(simplify_ring(&square(0.0, 0.0, 1.0)[..4], 1.0), None);
}

#[test]
fn zero_tolerance_keeps_every_corner_that_bends() {
    let mut ring = square(-100.0, 40.0, 0.01);
    // A midpoint on a straight edge is the only position that may go.
    ring.insert(1, [-99.995, 40.0]);
    let simplified = simplify_ring(&ring, 0.0).unwrap();
    assert_eq!(simplified, square(-100.0, 40.0, 0.01));
}

#[test]
fn tolerance_is_measured_on_the_ground_at_high_latitude() {
    // At 64°N a degree of longitude is ~48.7 km. A 0.004° east–west bump on a north–south
    // edge is ~195 m on the ground (but ~445 m if longitude were measured like latitude).
    let bump = [-144.946, 64.06];
    let ring = vec![
        [-145.0, 64.0],
        [-144.9, 64.0],
        [-144.9, 64.05],
        [-144.95, 64.05],
        bump,
        [-144.95, 64.07],
        [-145.0, 64.07],
        [-145.0, 64.0],
    ];
    let simplified = simplify_ring(&ring, 300.0).unwrap();
    assert!(!simplified.contains(&bump), "{simplified:?}");
    let detailed = simplify_ring(&ring, 100.0).unwrap();
    assert!(detailed.contains(&bump), "{detailed:?}");
}

#[test]
fn lines_keep_their_ends() {
    let line = vec![[0.0, 0.0], [0.5, 0.000_001], [1.0, 0.0]];
    assert_eq!(simplify_line(&line, 10.0), vec![[0.0, 0.0], [1.0, 0.0]]);
    assert_eq!(simplify_line(&line[..2], 10.0), line[..2].to_vec());
    assert_eq!(simplify_line(&line, 0.0), line);
}

#[test]
fn extent_size_and_vertex_mean() {
    let ring = square(-120.0, 0.0, 1.0);
    assert_eq!(extent(&ring), Some([-120.0, 0.0, -119.0, 1.0]));
    assert_eq!(extent(std::iter::empty()), None);
    let (width, height) = extent_size_m([-120.0, 60.0, -119.0, 60.0]);
    assert!((width - METRES_PER_DEGREE / 2.0).abs() < 1.0, "{width}");
    assert!(height.abs() < f64::EPSILON);
    assert_eq!(vertex_mean(&ring), Some([-119.5, 0.5]));
    assert_eq!(vertex_mean(&[]), None);
    assert_eq!(vertex_mean(&[[1.0, 2.0]]), Some([1.0, 2.0]));
}

/// Closed star-shaped rings around a centre: corners at increasing angles with random
/// radii, so every generated ring is simple (it never crosses itself).
fn star_ring() -> impl Strategy<Value = Vec<Position>> {
    (-170.0..170.0f64, -70.0..70.0f64, prop::collection::vec((0.2..1.0f64, 0.0..1.0f64), 3..80))
        .prop_map(|(lon, lat, corners)| {
            #[allow(clippy::cast_precision_loss)]
            let step = std::f64::consts::TAU / corners.len() as f64;
            let mut ring: Vec<Position> = corners
                .iter()
                .enumerate()
                .map(|(i, (radius, jitter))| {
                    #[allow(clippy::cast_precision_loss)]
                    let angle = (i as f64 + jitter * 0.9) * step;
                    [lon + 0.05 * radius * angle.cos(), lat + 0.05 * radius * angle.sin()]
                })
                .collect();
            ring.push(ring[0]);
            ring
        })
}

proptest! {
    #[test]
    fn simplified_rings_are_closed_in_order_subsets(ring in star_ring(), tolerance in 0.0..5_000.0f64) {
        let simplified = simplify_ring(&ring, tolerance).unwrap();
        prop_assert!(simplified.len() >= 4 && simplified.len() <= ring.len());
        prop_assert_eq!(simplified.first(), ring.first());
        prop_assert_eq!(simplified.first(), simplified.last());
        prop_assert!(kept_indices(&ring, &simplified).is_some());
    }

    #[test]
    fn every_removed_corner_lies_within_tolerance(ring in star_ring(), tolerance in 1.0..5_000.0f64) {
        let simplified = simplify_ring(&ring, tolerance).unwrap();
        let kept = kept_indices(&ring, &simplified).unwrap();
        let points = project(&ring);
        let limit = tolerance / METRES_PER_DEGREE * (1.0 + 1e-9);
        for i in (0..ring.len()).filter(|i| !kept.contains(i)) {
            let offset = kept
                .windows(2)
                .map(|pair| distance_to_segment(points[i], points[pair[0]], points[pair[1]]))
                .fold(f64::INFINITY, f64::min);
            prop_assert!(offset <= limit, "corner {} is {} from the outline", i, offset);
        }
    }

    #[test]
    fn coarser_tolerances_keep_a_subset(ring in star_ring(), fine in 0.0..2_000.0f64, extra in 0.0..3_000.0f64) {
        let detailed = simplify_ring(&ring, fine).unwrap();
        let coarse = simplify_ring(&ring, fine + extra).unwrap();
        prop_assert!(coarse.len() <= detailed.len());
        prop_assert!(kept_indices(&detailed, &coarse).is_some());
    }

    #[test]
    fn cleaning_simplified_rings_keeps_them_valid(ring in star_ring(), tolerance in 0.0..5_000.0f64) {
        let simplified = simplify_ring(&ring, tolerance).unwrap();
        let exterior = clean_ring(&simplified, true).unwrap();
        prop_assert!(signed_area(&exterior) > 0.0);
        let hole = clean_ring(&simplified, false).unwrap();
        prop_assert!(signed_area(&hole) < 0.0);
    }

    #[test]
    fn arbitrary_input_never_panics(
        positions in prop::collection::vec((-180.0..180.0f64, -90.0..90.0f64), 0..40),
        tolerance in -10.0..10_000.0f64,
    ) {
        let mut ring: Vec<Position> = positions.iter().map(|(x, y)| [*x, *y]).collect();
        let _ = simplify_ring(&ring, tolerance);
        let _ = simplify_line(&ring, tolerance);
        let _ = clean_ring(&ring, true);
        if let Some(first) = ring.first().copied() {
            ring.push(first);
        }
        if let Some(simplified) = simplify_ring(&ring, tolerance) {
            prop_assert!(simplified.len() >= 4);
        }
    }
}
