//! An R-tree over point positions for viewport queries on large stores (vessels, static
//! infrastructure). Antimeridian-crossing boxes are answered with two envelope queries.

use lavender_domain::{BBox, GeoPoint};
use rstar::primitives::GeomWithData;
use rstar::{AABB, RTree};

type Entry = GeomWithData<[f64; 2], usize>;

/// Indexes positions by their position in the caller's collection.
#[derive(Debug, Default)]
pub struct PointIndex {
    tree: RTree<Entry>,
}

impl PointIndex {
    /// Bulk-loads `(lon, lat)` points; each entry remembers its index in the input order.
    pub fn build<'a>(points: impl IntoIterator<Item = &'a GeoPoint>) -> Self {
        let entries: Vec<Entry> = points
            .into_iter()
            .enumerate()
            .map(|(i, p)| GeomWithData::new([p.lon, p.lat], i))
            .collect();
        Self { tree: RTree::bulk_load(entries) }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }

    /// Input indices of every point inside `bbox`, in ascending order.
    #[must_use]
    pub fn query(&self, bbox: &BBox) -> Vec<usize> {
        let mut hits: Vec<usize> = bbox
            .lon_ranges()
            .into_iter()
            .flat_map(|(west, east)| {
                let envelope = AABB::from_corners([west, bbox.south], [east, bbox.north]);
                self.tree.locate_in_envelope(envelope).map(|e| e.data).collect::<Vec<_>>()
            })
            .collect();
        hits.sort_unstable();
        hits.dedup();
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn finds_points_across_the_antimeridian() {
        let pts: Vec<GeoPoint> = [(0.0, 179.0), (0.0, -179.0), (0.0, 0.0), (45.0, 179.5)]
            .iter()
            .map(|&(lat, lon)| GeoPoint::new(lat, lon).unwrap())
            .collect();
        let idx = PointIndex::build(&pts);
        let b = BBox::new(178.0, -1.0, -178.0, 1.0).unwrap();
        assert_eq!(idx.query(&b), vec![0, 1]);
        assert_eq!(idx.query(&BBox::WORLD).len(), 4);
    }

    proptest! {
        #[test]
        fn agrees_with_linear_scan(
            pts in prop::collection::vec((-89.0f64..89.0, -179.0f64..179.0), 0..200),
            w in -180.0f64..180.0, e in -180.0f64..180.0, s in -90.0f64..0.0, n in 0.0f64..90.0,
        ) {
            let points: Vec<GeoPoint> = pts.iter().map(|&(lat, lon)| GeoPoint::new(lat, lon).unwrap()).collect();
            let b = BBox::new(w, s, e, n).unwrap();
            let expected: Vec<usize> = points.iter().enumerate().filter(|(_, p)| b.contains(p)).map(|(i, _)| i).collect();
            prop_assert_eq!(PointIndex::build(&points).query(&b), expected);
        }
    }
}
