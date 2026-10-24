//! MarXact spatial lookup with Python tie order and a corrected rounding boundary.
use crate::marxact::{MarXactLeader, LEADER_MATCH_TOLERANCE as TOLERANCE};
use std::collections::HashMap;

const SCALE: f64 = 1. / TOLERANCE;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum Cell {
    Integer(i64),
    // Beyond 2^53 scaled units, adjacent representable coordinates are already
    // farther apart than the tolerance. Preserve the actual coordinate bits;
    // a saturating integer cast or overflowing x*SCALE would merge distant cells.
    Large(u64),
}
impl Cell {
    fn at(value: f64) -> Self {
        let scaled = value * SCALE;
        if scaled.abs() < (1u64 << 53) as f64 {
            Self::Integer(scaled.round_ties_even() as i64)
        } else {
            Self::Large(value.to_bits())
        }
    }
    fn offset(self, amount: i64) -> Option<Self> {
        match self {
            Self::Integer(n) => Some(Self::Integer(n + amount)),
            Self::Large(_) if amount == 0 => Some(self),
            Self::Large(_) => None,
        }
    }
}
fn point_key(x: f64, y: f64) -> (u64, u64) {
    // Python treats negative and positive zero as the same coordinate.
    (
        if x == 0. { 0 } else { x.to_bits() },
        if y == 0. { 0 } else { y.to_bits() },
    )
}

pub struct LeaderIndex<'a> {
    leaders: &'a [MarXactLeader],
    buckets: HashMap<(Cell, Cell), usize>,
    links: Vec<Link>,
}
const END: usize = usize::MAX;
struct Link {
    source: usize,
    next: usize,
}
impl<'a> LeaderIndex<'a> {
    pub fn new(leaders: &'a [MarXactLeader]) -> Self {
        let mut last = HashMap::with_capacity(leaders.len());
        for (index, leader) in leaders.iter().enumerate() {
            if leader.x.is_finite() && leader.y.is_finite() {
                last.insert(point_key(leader.x, leader.y), index);
            }
        }
        // One contiguous array avoids a heap allocation for every occupied cell.
        let mut buckets = HashMap::with_capacity(last.len());
        let mut links = Vec::with_capacity(last.len());
        for (index, leader) in leaders.iter().enumerate() {
            if last.get(&point_key(leader.x, leader.y)) != Some(&index) {
                continue;
            }
            // Equal coordinates have identical distances for every query, so
            // only their last label can win. Store source order for other ties.
            let next = buckets
                .insert((Cell::at(leader.x), Cell::at(leader.y)), links.len())
                .unwrap_or(END);
            links.push(Link {
                source: index,
                next,
            });
        }
        Self {
            leaders,
            buckets,
            links,
        }
    }
    pub fn nearest(&self, x: f64, y: f64) -> Option<&'a MarXactLeader> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (base_x, base_y) = (Cell::at(x), Cell::at(y));
        let mut best = None;
        let mut best_distance = TOLERANCE * TOLERANCE;
        // Order is part of the Python contract: later equal-distance candidates
        // win, including across different buckets, independent of hash order.
        // Ties-to-even can put points exactly one tolerance apart two cells
        // apart (0.04 -> cell 0, 0.12 -> cell 2). The original 3x3 search
        // silently missed these labels. Two-cell padding also covers rounding
        // in the coordinate-to-cell multiplication. Distance remains decisive.
        for dx in -2..=2 {
            let Some(cx) = base_x.offset(dx) else {
                continue;
            };
            for dy in -2..=2 {
                let Some(cy) = base_y.offset(dy) else {
                    continue;
                };
                if let Some(&head) = self.buckets.get(&(cx, cy)) {
                    let mut cursor = head;
                    let mut chosen = None;
                    let mut nearest_distance = best_distance;
                    while cursor != END {
                        let Link {
                            source: index,
                            next,
                        } = self.links[cursor];
                        let leader = &self.leaders[index];
                        let distance = (leader.x - x).powi(2) + (leader.y - y).powi(2);
                        // Chains are reversed; source index restores Python's
                        // last-candidate rule within each cell without sorting.
                        if distance < nearest_distance
                            || (distance == nearest_distance && chosen.is_none_or(|i| index > i))
                        {
                            chosen = Some(index);
                            nearest_distance = distance;
                        }
                        cursor = next;
                    }
                    if let Some(index) = chosen {
                        best = Some(&self.leaders[index]);
                        best_distance = nearest_distance;
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn leader(x: f64, y: f64, name: &str) -> MarXactLeader {
        MarXactLeader {
            x,
            y,
            name: name.into(),
            height: None,
        }
    }
    #[test]
    fn coincident_labels_collapse_to_last_without_reordering_other_ties() {
        let mut leaders = vec![leader(0., -0., "duplicate"); 100_000];
        leaders.push(leader(-0., 0., "last"));
        let index = LeaderIndex::new(&leaders);
        assert_eq!(index.links.len(), 1);
        assert_eq!(index.nearest(0., 0.).unwrap().name, "last");
        assert_eq!(index.nearest(0.08, 0.).unwrap().name, "last");
        assert!(index.nearest(0.081, 0.).is_none());
    }
    #[test]
    fn huge_finite_coordinates_are_distinct_and_invalid_values_do_not_match() {
        let leaders = [
            leader(f64::MAX, 0., "maximum"),
            leader(1e30, -1e30, "large"),
            leader(f64::NAN, 0., "invalid"),
        ];
        let index = LeaderIndex::new(&leaders);
        assert_eq!(index.nearest(f64::MAX, 0.).unwrap().name, "maximum");
        assert_eq!(index.nearest(1e30, -1e30).unwrap().name, "large");
        assert!(index.nearest(2e30, -1e30).is_none());
        assert!(index.nearest(f64::INFINITY, 0.).is_none());
        assert!(index.nearest(0., f64::NAN).is_none());
    }
}
