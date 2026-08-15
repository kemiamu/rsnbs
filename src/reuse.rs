//! Reuse-maximizing layer flow, ported from `wf_0813_reuse/reuse_flow.py`.
//!
//! Scientific basis (KNOWLEDGE.md §3.30-3.36):
//!
//! - §3.30 anchor-chain law: for AP scatter `{0,d,...,(n-1)d}` the conflict
//!   graph decomposes into disjoint anchor chains; the greedy kernel takes
//!   `ceil(L/n)` anchors per chain (L = chain length in len-2 anchors).
//! - §3.31 per-chain closed form: a chain covering T ticks yields
//!   `f(T) = 3*floor(T/4) + [0,0,1,2][T mod 4]` reuse under deep-first order
//!   (len 4 -> 3 -> 2); deep-first is the unique optimal order within a family.
//! - §3.33 nested penalty: a len-2 pair nested inside a finer family's block
//!   forfeits (k-1) fine gain — used only for family ranking.
//! - §3.34 density arbitration: ranking key = (score, deepest len used, -d).
//! - §3.36 optimality: cross-free (chain-structured) inputs are solved exactly
//!   by the chain decomposition + f(T); general inputs are NP-hard (3-AP
//!   packing), so the greedy (97%) + short augmenting (98.8%) is the natural
//!   approximation at the hardness boundary.

use crate::note::Tone;
use crate::types::Tick;
use crate::util::{Point, TpPlane, TransEqClass};
use itertools::Itertools;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZero;

// Layer
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single reuse layer: an offset set paired with its capacity-safe kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// Translation offsets (scatter, ascending, always contains 0).
    pub scatter: Vec<Tick>,
    /// Capacity-safe arithmetic kernel (anchor point -> multiplicity).
    pub kernel: TpPlane,
}

impl Layer {
    /// Reuse of this layer: `sum(K) * (|S| - 1)`.
    pub fn reuse(&self) -> usize {
        self.kernel.values().sum::<usize>() * (self.scatter.len() - 1)
    }
}

// Multiset helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Add `count` to the multiplicity of `event` in `plane`.
fn add_to(plane: &mut TpPlane, event: Point, count: usize) {
    plane
        .entry(event)
        .and_modify(|mult| *mult += count)
        .or_insert(count);
}

/// Add `count` to the support of `offset`.
fn add_count(support: &mut BTreeMap<Tick, usize>, offset: Tick, count: usize) {
    *support.entry(offset).or_default() += count;
}

// Expansion
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Expand `K (+) S`: kernel translated by every offset, multiplicities summed.
pub fn expand(kernel: &TpPlane, scatter: &[Tick]) -> TpPlane {
    let mut out = TpPlane::default();
    for &offset in scatter {
        for (&(tick, tone), &count) in kernel.iter() {
            if count > 0 {
                add_to(&mut out, (tick + offset, tone), count);
            }
        }
    }
    out
}

/// Subtract `consumed` from `source`, panicking if coverage exceeds source
/// multiplicity (mirrors `subtract_exact`'s `ValueError`).
pub fn subtract_exact(source: &TpPlane, consumed: &TpPlane) -> TpPlane {
    let mut out = source.clone();
    for (event, count) in consumed.iter() {
        let have = out.get(event).copied().unwrap_or(0);
        assert!(
            have >= *count,
            "coverage exceeds source multiplicity at {event:?}"
        );
        out.entry(*event).and_modify(|mult| *mult -= count);
    }
    out.retain(|_, mult| *mult > 0);
    out
}

// Autocorrelation
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multiset match upper bound for every positive time offset.
///
/// Per-tone pair enumeration: `support[right - left] += min(lc, rc)`.
pub fn autocorrelation(source: &TpPlane) -> BTreeMap<Tick, usize> {
    let mut by_tone: BTreeMap<Tone, BTreeMap<Tick, usize>> = BTreeMap::new();
    for (&(tick, tone), &count) in source.iter() {
        if count > 0 {
            by_tone.entry(tone).or_default().insert(tick, count);
        }
    }

    let mut support: BTreeMap<Tick, usize> = BTreeMap::new();
    for timeline in by_tone.values() {
        let ticks: Vec<Tick> = timeline.keys().copied().collect();
        for (index, &left) in ticks.iter().enumerate() {
            for &right in &ticks[index + 1..] {
                let matched = timeline[&left].min(timeline[&right]);
                add_count(&mut support, right - left, matched);
            }
        }
    }
    support
}

// Kernel
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Construct a capacity-safe kernel for the given scatter.
///
/// Per-tone greedy least-conflict anchor selection (conflict graph
/// independent set): commit the minimum multiplicity of the least-conflicting
/// anchor, deduct it, and repeat. `expand(kernel, scatter)` is always within
/// `source` (a heuristic can miss savings but never overdraws the source).
pub fn feasible_kernel(source: &TpPlane, scatter: &[Tick]) -> TpPlane {
    let offsets: Vec<Tick> = scatter.iter().copied().sorted().dedup().collect();
    assert!(offsets.contains(&0), "scatter must contain zero");

    let mut by_tone: BTreeMap<Tone, BTreeMap<Tick, usize>> = BTreeMap::new();
    for (&(tick, tone), &count) in source.iter() {
        if count > 0 {
            by_tone.entry(tone).or_default().insert(tick, count);
        }
    }

    let mut kernel = TpPlane::default();
    for (tone, capacities) in &by_tone {
        kernel_for_tone(tone, capacities, &offsets, &mut kernel);
    }
    kernel
}

/// Per-tone greedy independent set over the anchor conflict graph.
fn kernel_for_tone(
    tone: &Tone,
    capacities: &BTreeMap<Tick, usize>,
    offsets: &[Tick],
    kernel: &mut TpPlane,
) {
    let mut available = capacities.clone();

    // Anchors: points whose full offset neighborhood still has capacity.
    let anchors: BTreeSet<Tick> = capacities
        .keys()
        .copied()
        .filter(|&anchor| has_full_neighborhood(capacities, offsets, anchor))
        .collect();

    // Anchor -> the ticks it covers (anchor + each offset).
    let covered: BTreeMap<Tick, Vec<Tick>> = anchors
        .iter()
        .map(|&anchor| (anchor, covered_ticks(anchor, offsets)))
        .collect();

    // Tick -> anchors covering it.
    let mut by_tick: BTreeMap<Tick, BTreeSet<Tick>> = BTreeMap::new();
    for (&anchor, ticks) in &covered {
        for &tick in ticks {
            by_tick.entry(tick).or_default().insert(anchor);
        }
    }

    // Least-conflict first (conflict count excluding self), ties by anchor.
    let mut ranked: BTreeSet<(usize, Tick)> = BTreeSet::new();
    for (&anchor, ticks) in &covered {
        let conflicts = conflicting_anchors(&by_tick, ticks);
        ranked.insert((conflicts.len() - 1, anchor));
    }

    let mut active: BTreeSet<Tick> = anchors;
    while let Some(&key) = ranked.iter().next() {
        ranked.remove(&key);
        let (_, anchor) = key;
        if !active.contains(&anchor) {
            continue;
        }
        let ticks = &covered[&anchor];
        commit_anchor(kernel, *tone, anchor, ticks, &mut available);
        active.remove(&anchor);
        for &tick in ticks {
            expire_depleted(tick, &by_tick, &available, &mut active);
        }
    }
}

/// Whether every offset neighborhood of `anchor` still has capacity.
fn has_full_neighborhood(
    capacities: &BTreeMap<Tick, usize>,
    offsets: &[Tick],
    anchor: Tick,
) -> bool {
    offsets
        .iter()
        .all(|&offset| capacities.get(&(anchor + offset)).copied().unwrap_or(0) > 0)
}

/// Ticks covered by an anchor: anchor + each offset.
fn covered_ticks(anchor: Tick, offsets: &[Tick]) -> Vec<Tick> {
    offsets.iter().map(|&offset| anchor + offset).collect()
}

/// Anchors sharing at least one covered tick with `ticks`.
fn conflicting_anchors(by_tick: &BTreeMap<Tick, BTreeSet<Tick>>, ticks: &[Tick]) -> BTreeSet<Tick> {
    let mut conflicts = BTreeSet::new();
    for &tick in ticks {
        let Some(anchors) = by_tick.get(&tick) else {
            continue;
        };
        conflicts.extend(anchors);
    }
    conflicts
}

/// Commit the least-conflicting anchor: write its minimum multiplicity into
/// the kernel and deduct it from the available capacities.
fn commit_anchor(
    kernel: &mut TpPlane,
    tone: Tone,
    anchor: Tick,
    ticks: &[Tick],
    available: &mut BTreeMap<Tick, usize>,
) {
    let count = ticks
        .iter()
        .map(|tick| available.get(tick).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    if count == 0 {
        return;
    }
    add_to(kernel, (anchor, tone), count);
    for &tick in ticks {
        let Some(cap) = available.get_mut(&tick) else {
            continue;
        };
        *cap -= count;
    }
}

/// Drop anchors whose covered ticks are fully consumed from the active set.
fn expire_depleted(
    tick: Tick,
    by_tick: &BTreeMap<Tick, BTreeSet<Tick>>,
    available: &BTreeMap<Tick, usize>,
    active: &mut BTreeSet<Tick>,
) {
    if available.get(&tick).copied().unwrap_or(0) > 0 {
        return;
    }
    let Some(anchors) = by_tick.get(&tick) else {
        return;
    };
    active.retain(|a| !anchors.contains(a));
}

// Family deep-first
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Forfeited fine-block gain if this family's len-2 pairs are committed.
///
/// A pair `(a, a+d)` nested in a finer block at spacing `d/k` (k in {2,3})
/// yields k when the finer family takes it instead of 1 here, so committing
/// the pair forfeits (k-1). Used only to rank families for arbitration;
/// committed layers keep their full kernels.
pub fn nested_penalty(kernel: &TpPlane, d: Tick, work: &TpPlane) -> usize {
    let mut penalty = 0;
    for (&(anchor, tone), &count) in kernel.iter() {
        for k in [2u32, 3] {
            if d % k != 0 {
                continue;
            }
            let step = d / k;
            let nested =
                (1..k).all(|i| work.get(&(anchor + i * step, tone)).copied().unwrap_or(0) > 0);
            if nested {
                penalty += (k as usize - 1) * count;
                break;
            }
        }
    }
    penalty
}

/// Deep-first (len decreasing) layers of the AP family at spacing `d`.
///
/// Returns `(total_reuse, layers, nested_penalty)` where layers are ordered
/// 4, 3, 2 and each entry is a [`Layer`]. The penalty estimates forfeited
/// fine-block gain on the family's len-2 pairs and is used only for family
/// arbitration.
pub fn family_deep_first(
    source: &TpPlane,
    d: Tick,
    max_len: usize,
    budget: usize,
) -> (usize, Vec<Layer>, usize) {
    let mut total = 0;
    let mut layers = Vec::new();
    let mut work = source.clone();
    let mut penalty = 0;

    for n in (2..=max_len).rev() {
        if layers.len() >= budget {
            break;
        }
        let scatter: Vec<Tick> = (0..n).map(|i| (i as Tick) * d).collect();
        let kernel = feasible_kernel(&work, &scatter);
        if n == 2 {
            penalty = nested_penalty(&kernel, d, &work);
        }
        let gain = kernel.values().sum::<usize>() * (n - 1);
        if gain <= 0 {
            continue;
        }
        total += gain;
        let expansion = expand(&kernel, &scatter);
        layers.push(Layer { scatter, kernel });
        work = subtract_exact(&work, &expansion);
    }
    (total, layers, penalty)
}

// Reuse flow
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Greedy family arbitration of the deep-first family flow.
///
/// Returns `(plan, total_reuse, residual)`. Each round scores every surviving
/// AP family with its complete deep-first flow, then commits the winner
/// (`score = total - nested_penalty`, deepest len used, smallest d).
pub fn reuse_flow(
    source: &TpPlane,
    max_len: usize,
    max_layers: usize,
) -> (Vec<Layer>, usize, TpPlane) {
    let mut residual = source.clone();
    let mut plan: Vec<Layer> = Vec::new();
    let mut total_reuse = 0;

    while plan.len() < max_layers {
        let support = autocorrelation(&residual);
        let Some(&max_support) = support.values().max() else {
            break;
        };
        // Non-AP candidates die on this threshold (sup anti-monotone class).
        let threshold = max_support / (max_len - 1);
        let candidates: Vec<Tick> = support
            .iter()
            .filter(|&(_, &value)| value > threshold)
            .map(|(&offset, _)| offset)
            .collect();
        if candidates.is_empty() {
            break;
        }

        let mut best: Option<((usize, usize, Reverse<Tick>), Vec<Layer>)> = None;
        for &d in &candidates {
            let budget = max_layers - plan.len();
            let (total, layers, penalty) = family_deep_first(&residual, d, max_len, budget);
            if total <= 0 {
                continue;
            }
            let score = total - penalty;
            let deepest = layers
                .iter()
                .map(|layer| layer.scatter.len())
                .max()
                .unwrap_or(0);
            let key = (score, deepest, Reverse(d));
            if best.as_ref().is_none_or(|(best_key, _)| key > *best_key) {
                best = Some((key, layers));
            }
        }
        let Some((_, layers)) = best else {
            break;
        };

        for layer in layers {
            let expansion = expand(&layer.kernel, &layer.scatter);
            total_reuse += layer.reuse();
            residual = subtract_exact(&residual, &expansion);
            plan.push(layer);
        }
    }

    (plan, total_reuse, residual)
}

// Layout adaptation
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Convert a reuse plan and residual into layout TECs.
///
/// Layers whose minimum offset gap is too tight for the tapped delay line
/// (repeater coarse >= 4, i.e. min gap >= 8) are skipped and absorbed back
/// into the residual (degenerate absorption).
///
/// Returns `(tecs, skipped_layers)` where the residual is appended as a
/// no-offset TEC when non-empty.
pub fn plan_to_tecs(plan: Vec<Layer>, mut residual: TpPlane) -> (Vec<TransEqClass>, usize) {
    let mut tecs = Vec::new();
    let mut skipped = 0;
    for layer in plan {
        let min_gap = layer.scatter.windows(2).map(|w| w[1] - w[0]).min();
        if min_gap < Some(8) {
            let expansion = expand(&layer.kernel, &layer.scatter);
            for (event, count) in expansion.iter() {
                add_to(&mut residual, *event, *count);
            }
            skipped += 1;
            continue;
        }
        let offsets: BTreeSet<NonZero<Tick>> = layer
            .scatter
            .into_iter()
            .skip(1)
            .filter_map(NonZero::new)
            .collect();
        tecs.push(TransEqClass::new(offsets, layer.kernel));
    }
    if !residual.is_empty() {
        tecs.push(TransEqClass::new(BTreeSet::new(), residual));
    }
    (tecs, skipped)
}

// Tests
//
// ++++++++++++============++++++++++++============++++++++++++============

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::{Instrument, Key, Tone};

    fn tone() -> Tone {
        Tone::new(Instrument::Harp, Key::FS3)
    }

    fn chain(n: usize, d: Tick, start: Tick) -> TpPlane {
        TpPlane::from_iter((0..n).map(|i| (start + (i as Tick) * d, tone())))
    }

    /// Per-chain closed form: `f(T) = 3*floor(T/4) + [0,0,1,2][T mod 4]`.
    fn f(ticks: usize) -> usize {
        3 * (ticks / 4) + [0, 0, 1, 2][ticks % 4]
    }

    /// Composition check: `expand(plan) + residual == source`.
    fn verify_composition(source: &TpPlane, plan: &[Layer], residual: &TpPlane) {
        let mut total = residual.clone();
        for layer in plan {
            let expanded = expand(&layer.kernel, &layer.scatter);
            for (event, count) in expanded.iter() {
                add_to(&mut total, *event, *count);
            }
        }
        assert_eq!(&total, source);
    }

    #[test]
    fn family_deep_first_matches_chain_closed_form() {
        for ticks in 2..16 {
            let (total, layers, _) = family_deep_first(&chain(ticks, 128, 1000), 128, 4, 3);
            assert_eq!(total, f(ticks), "T={ticks}");
            let lens: Vec<usize> = layers.iter().map(|layer| layer.scatter.len()).collect();
            let mut sorted = lens.clone();
            sorted.sort_by(|a, b| b.cmp(a));
            assert_eq!(lens, sorted, "T={ticks}");
        }
    }

    #[test]
    fn deep_first_beats_single_only_on_long_chain() {
        let (total, _, _) = family_deep_first(&chain(12, 128, 1000), 128, 4, 3);
        assert_eq!(total, 9);
        assert!(total > 6); // single-offset alone gives ceil(11/2)=6
    }

    #[test]
    fn non_ap_motif_deep_wins() {
        let t = tone();
        let m = TpPlane::from_iter([
            (0u32, t),
            (100u32, t),
            (1000u32, t),
            (2000u32, t),
            (2100u32, t),
            (3000u32, t),
        ]);
        let (plan, total, residual) = reuse_flow(&m, 4, 4);
        assert!(total > 3); // best single offset gives 3
        assert!(residual.is_empty());
        verify_composition(&m, &plan, &residual);
    }

    #[test]
    fn flow_consumes_no_more_than_source() {
        let m = chain(10, 128, 1000);
        let (plan, _, residual) = reuse_flow(&m, 4, 4);
        verify_composition(&m, &plan, &residual);
    }

    #[test]
    fn isolated_chains_stay_single_offset() {
        let t = tone();
        let m = TpPlane::from_iter([(0u32, t), (128u32, t), (4000u32, t), (4128u32, t)]);
        let (_, total, _) = reuse_flow(&m, 4, 4);
        assert_eq!(total, 2); // two isolated pairs, no deep layer possible
    }

    #[test]
    fn arbitration_matches_exhaustive_optimum() {
        let cases: &[(&[Tick], usize)] = &[
            (&[7, 9, 19, 34, 35, 36, 52], 4),
            (&[4, 7, 10, 26, 32, 38], 4),
            (&[3, 4, 19, 42, 44, 46, 59], 4),
            (&[7, 29, 30, 31, 54, 57, 59], 4),
            (&[7, 11, 12, 24, 30, 45, 48, 50, 58], 5),
            (&[22, 26, 29, 32, 33, 37, 42, 52, 57], 6),
            (&[8, 9, 16, 26, 27, 29, 35, 61, 71], 5),
            (&[8, 9, 17, 39, 40, 57, 75], 4),
            (&[10, 56, 58, 59, 66, 69, 74], 4),
        ];
        for (ticks, expected) in cases {
            let t = tone();
            let m = TpPlane::from_iter(ticks.iter().map(|&x| (x, t)));
            let (_, total, _) = reuse_flow(&m, 4, 1000);
            assert_eq!(total, *expected, "ticks={ticks:?}");
        }
    }
}
