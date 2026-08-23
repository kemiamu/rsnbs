//! Translation analysis of the time/event plane.
//!
//! The theoretical core of the formal reduction `M = K (+) S + R`: base
//! types and abstractions live here, while decomposition algorithms live
//! in the [`reuse`] submodule.

use crate::note::Notes;
use crate::types::{Tick, TimeAnchor};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::repeat;
use std::num::NonZero;
use std::ops::{BitAnd, Deref, DerefMut};

pub mod reuse;

// Event
//
// ++++++++++++============++++++++++++============++++++++++++============

/// The plane's second axis: an event occurring at a tick.
///
/// In the design document this is a tone, which may be any enum type;
/// `note::Notes` uses the same word for its value type. `Event` is the
/// minimal capability set the plane and TEC machinery need from it.
pub trait Event: Hash + Eq + Ord + Clone + Debug {}
impl<T: Hash + Eq + Ord + Clone + Debug> Event for T {}

// TePlane
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A point in the TE (time/event) plane.
pub type Point<E> = (Tick, E);

/// TE (time/event) plane multiset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TePlane<E: Event>(Counter<Point<E>>);

impl<E: Event> Default for TePlane<E> {
    fn default() -> Self {
        TePlane(Counter::new())
    }
}

impl<E: Event> TePlane<E> {
    /// Expand into an iterator of individual `(Tick, Event)` points.
    pub fn into_points(self) -> impl Iterator<Item = Point<E>> {
        let TePlane(inner) = self;
        inner
            .into_iter()
            .flat_map(|(point, count)| repeat(point).take(count))
    }

    /// Shift every point by `offset`, expanding multiplicity.
    pub fn translated(&self, offset: Tick) -> impl Iterator<Item = Point<E>> {
        self.iter().flat_map(move |(&(tick, ref tone), &count)| {
            repeat((tick + offset, tone.clone())).take(count)
        })
    }
}

/// Collects `TE(time, event)` points into a plane.
impl<T: TimeAnchor, E: Into<U>, U: Event> FromIterator<(T, E)> for TePlane<U> {
    fn from_iter<I: IntoIterator<Item = (T, E)>>(iter: I) -> Self {
        let inner = iter.into_iter().map(|(t, e)| (t.into_tick(), e.into()));
        Self(inner.collect())
    }
}

impl<E: Event> From<TePlane<E>> for Notes<Tick, Vec<E>> {
    fn from(plane: TePlane<E>) -> Self {
        let by_tick = plane.into_points().into_group_map();
        let notes = by_tick.into_iter().map(|(tick, mut tones)| {
            tones.sort_unstable();
            (tick, tones)
        });
        notes.collect()
    }
}

impl<E: Event> Deref for TePlane<E> {
    type Target = Counter<Point<E>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: Event> DerefMut for TePlane<E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Translation Equivalence Class
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Translation Equivalence Class (TEC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransEqClass<E: Event> {
    /// Translation offsets (scatter, ascending, excluding 0: zero is implied).
    pub scatter: BTreeSet<NonZero<Tick>>,
    /// Points (multiset) that this TEC operates on.
    pub kernel: TePlane<E>,
}

impl<E: Event> TransEqClass<E> {
    pub fn new(scatter: BTreeSet<NonZero<Tick>>, kernel: TePlane<E>) -> Self {
        Self { scatter, kernel }
    }

    /// Reuse gain: `sum(kernel) * scatter.len()`. The scatter excludes the
    /// implied zero offset, so its length is `|S| - 1`.
    pub fn reuse(&self) -> usize {
        self.kernel.values().sum::<usize>() * self.scatter.len()
    }

    /// Expand `kernel (+) scatter` into a plane, including the implied zero offset.
    pub fn expand(&self) -> TePlane<E> {
        std::iter::once(0)
            .chain(self.scatter.iter().map(|o| o.get()))
            .flat_map(|offset| self.kernel.translated(offset))
            .collect()
    }

    /// Minimum gap between adjacent offsets, including the implied zero.
    pub fn min_gap(&self) -> Option<Tick> {
        std::iter::once(0)
            .chain(self.scatter.iter().map(|o| o.get()))
            .array_windows::<2>()
            .map(|[a, b]| b - a)
            .min()
    }
}

impl<E: Event> BitAnd for TransEqClass<E> {
    fn bitand(self, rhs: Self) -> Self {
        let scatter = &self.scatter | &rhs.scatter;
        let kernel = TePlane(self.kernel.0 & rhs.kernel.0);
        Self { scatter, kernel }
    }
    type Output = Self;
}

// Bounded TEC
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A TEC whose kernel expansion stays within its points:
/// `kernel (+) scatter <= points`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedTec<E: Event>(TransEqClass<E>);

impl<E: Event> BoundedTec<E> {
    /// Deducts each point's covered multiplicity from its shifted copies,
    /// keeping the kernel expansion within the TEC's points.
    pub fn new(mut tec: TransEqClass<E>) -> Self {
        let indexes: Vec<Point<E>> = tec.kernel.keys().cloned().sorted().collect();
        for (point, scatter_offset) in iproduct!(indexes, tec.scatter.iter()) {
            let anchor_mult = tec.kernel[&point];
            let (tick, tone) = point;
            let shifted = (tick + scatter_offset.get(), tone);
            let entry = tec.kernel.entry(shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        BoundedTec(tec)
    }

    /// Unwrap into the underlying (already bounded) TEC.
    pub fn into_inner(self) -> TransEqClass<E> {
        self.0
    }
}

impl<E: Event> Deref for BoundedTec<E> {
    type Target = TransEqClass<E>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
