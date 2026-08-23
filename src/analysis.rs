//! Translation analysis of the time/event plane.
//!
//! The theoretical core of the formal reduction `M = K (+) S + R`: base
//! types and abstractions live here, while decomposition algorithms live
//! in the [`reuse`] submodule.

use crate::note::Notes;
use crate::types::{Tick, TimeAnchor};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::{BTreeMap, BTreeSet};
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
        self.iter().flat_map(move |(&(tick, ref event), &count)| {
            repeat((tick + offset, event.clone())).take(count)
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

    /// Offsets including the implied zero, ascending.
    pub fn offsets(&self) -> impl Iterator<Item = Tick> {
        std::iter::once(0).chain(self.scatter.iter().map(|o| o.get()))
    }

    /// Number of offsets, including the implied zero.
    pub fn arity(&self) -> usize {
        self.scatter.len() + 1
    }

    /// Reuse gain: `sum(kernel) * scatter.len()`. The scatter excludes the
    /// implied zero offset, so its length is `|S| - 1`.
    pub fn reuse(&self) -> usize {
        self.kernel.values().sum::<usize>() * self.scatter.len()
    }

    /// Total covered multiplicity: `sum(kernel) * (scatter.len() + 1)`.
    pub fn coverage(&self) -> usize {
        self.kernel.values().sum::<usize>() * self.arity()
    }

    /// Expand `kernel (+) scatter` into a plane, including the implied zero offset.
    pub fn expand(&self) -> TePlane<E> {
        self.offsets()
            .flat_map(|offset| self.kernel.translated(offset))
            .collect()
    }

    /// Minimum gap between adjacent offsets, including the implied zero.
    pub fn min_gap(&self) -> Option<Tick> {
        self.offsets()
            .array_windows::<2>()
            .map(|[a, b]| b - a)
            .min()
    }

    /// Translate the kernel along the time axis; the scatter is unchanged.
    pub fn translate(self, tick: Tick) -> Self {
        let Self { scatter, kernel } = self;
        let kernel = kernel.translated(tick).collect();
        Self { kernel, scatter }
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
    ///
    /// The pruning is lossy: it trades precision for performance and lower
    /// mental overhead, with the arithmetic constraint carried by the type.
    pub fn new(mut tec: TransEqClass<E>) -> Self {
        let indexes: Vec<Point<E>> = tec.kernel.keys().cloned().sorted().collect();
        for (point, scatter_offset) in iproduct!(indexes, tec.scatter.iter()) {
            let anchor_mult = tec.kernel[&point];
            let (tick, event) = point;
            let shifted = (tick + scatter_offset.get(), event);
            let entry = tec.kernel.entry(shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        BoundedTec(tec)
    }

    /// Extract a bounded TEC from `source` under `scatter` by directed
    /// stepwise deconvolution, leaving the residual in `source`.
    ///
    /// Precision is lower than conflict-based allocation: committing in
    /// ascending order lets early deductions shape later slots. It performs
    /// well on hot paths. Assumes a linear time axis: offsets never shift
    /// backward, so the frontier advances monotonically. Does not apply to
    /// cyclic spaces where the axis wraps around.
    pub fn extract_from(source: &mut TePlane<E>, scatter: BTreeSet<NonZero<Tick>>) -> Self {
        let plane = std::mem::take(source);
        let offsets: Vec<Tick> = scatter.iter().map(|o| o.get()).collect();
        let mut work: BTreeMap<(Tick, &E), usize> = plane
            .iter()
            .map(|(&(tick, ref event), &count)| ((tick, event), count))
            .collect();

        let mut kernel = TePlane::default();
        while let Some((_point @ (tick, event), capacity)) = work.pop_first() {
            let slots = offsets.iter().map(|&offset| {
                let cap = work.get(&(tick + offset, event));
                cap.copied().unwrap_or(0)
            });
            let base = slots.fold(capacity, usize::min);
            let rest = capacity - base;
            if rest > 0 {
                source.entry((tick, event.clone())).or_insert(rest);
            }
            if base == 0 {
                continue;
            }
            for &offset in &offsets {
                *work.get_mut(&(tick + offset, event)).unwrap() -= base;
            }
            let anchor = kernel.entry((tick, event.clone()));
            anchor.and_modify(|mult| *mult += base).or_insert(base);
        }

        Self(TransEqClass { scatter, kernel })
    }

    /// Non-consuming `extract_from`: clones `source` and returns the
    /// extracted TEC, leaving `source` unchanged.
    pub fn extract(source: &TePlane<E>, scatter: BTreeSet<NonZero<Tick>>) -> Self {
        let mut source = source.clone();
        Self::extract_from(&mut source, scatter)
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
