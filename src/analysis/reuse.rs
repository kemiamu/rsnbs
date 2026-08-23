//! Reuse-maximizing layer flow, ported from `wf_0813_reuse/reuse_flow.py`.
//!
//! **Experimental**: the decomposition algorithm and its layout adaptation
//! are still under development; the output may change.
//!
//! Scientific basis (KNOWLEDGE.md 3.30-3.36):
//!
//! - 3.30 anchor-chain law: for AP scatter `{0,d,...,(n-1)d}` the conflict
//!   graph decomposes into disjoint anchor chains; the greedy kernel takes
//!   `ceil(L/n)` anchors per chain (L = chain length in len-2 anchors).
//! - 3.31 per-chain closed form: a chain covering T ticks yields
//!   `f(T) = 3*floor(T/4) + [0,0,1,2][T mod 4]` reuse under deep-first order
//!   (len 4 -> 3 -> 2); deep-first is the unique optimal order within a family.
//! - 3.33 nested penalty: a len-2 pair nested inside a finer family's block
//!   forfeits (k-1) fine gain (used only for family ranking).
//! - 3.34 density arbitration: ranking key = (score, deepest len used, -d).
//! - 3.36 optimality: cross-free (chain-structured) inputs are solved exactly
//!   by the chain decomposition + f(T); general inputs are NP-hard (3-AP
//!   packing), so the greedy (97%) + short augmenting (98.8%) is the natural
//!   approximation at the hardness boundary.

use crate::analysis::{BoundedTec, Event, Point, TePlane, TransEqClass};
use crate::types::Tick;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZero;

// Multiset helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Add `count` to the multiplicity of `event` in `plane`.
fn add_to<E: Event>(plane: &mut TePlane<E>, event: Point<E>, count: usize) {
    plane
        .entry(event)
        .and_modify(|mult| *mult += count)
        .or_insert(count);
}

/// Add `count` to the support of `offset`.
fn add_count(support: &mut BTreeMap<Tick, usize>, offset: Tick, count: usize) {
    *support.entry(offset).or_default() += count;
}

// Subtraction
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Subtract `consumed` from `source`, panicking if coverage exceeds source
/// multiplicity (mirrors `subtract_exact`'s `ValueError`).
pub fn subtract_exact<E: Event>(source: &TePlane<E>, consumed: &TePlane<E>) -> TePlane<E> {
    let mut out = source.clone();
    for (event, count) in consumed.iter() {
        let have = out.get(event).copied().unwrap_or(0);
        assert!(
            have >= *count,
            "coverage exceeds source multiplicity at {event:?}"
        );
        out.entry(event.clone()).and_modify(|mult| *mult -= count);
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
pub fn autocorrelation<E: Event>(source: &TePlane<E>) -> BTreeMap<Tick, usize> {
    let mut by_event: BTreeMap<E, BTreeMap<Tick, usize>> = BTreeMap::new();
    for (&(tick, ref event), &count) in source.iter() {
        if count > 0 {
            by_event
                .entry(event.clone())
                .or_default()
                .insert(tick, count);
        }
    }

    let mut support: BTreeMap<Tick, usize> = BTreeMap::new();
    for timeline in by_event.values() {
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

// Family deep-first
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Forfeited fine-block gain if this family's len-2 pairs are committed.
///
/// A pair `(a, a+d)` nested in a finer block at spacing `d/k` (k in {2,3})
/// yields k when the finer family takes it instead of 1 here, so committing
/// the pair forfeits (k-1). Used only to rank families for arbitration;
/// committed layers keep their full kernels.
pub fn nested_penalty<E: Event>(kernel: &TePlane<E>, d: Tick, work: &TePlane<E>) -> usize {
    let mut penalty = 0;
    for (&(anchor, ref event), &count) in kernel.iter() {
        for k in [2u32, 3] {
            if d % k != 0 {
                continue;
            }
            let step = d / k;
            let nested = (1..k).all(|i| {
                work.get(&(anchor + i * step, event.clone()))
                    .copied()
                    .unwrap_or(0)
                    > 0
            });
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
/// 4, 3, 2 and each entry is a [`TransEqClass`]. The penalty estimates
/// forfeited fine-block gain on the family's len-2 pairs and is used only
/// for family arbitration.
pub fn family_deep_first<E: Event>(
    source: &TePlane<E>,
    d: Tick,
    max_len: usize,
    budget: usize,
) -> (usize, Vec<TransEqClass<E>>, usize) {
    let mut total = 0;
    let mut layers = Vec::new();
    let mut work = source.clone();
    let mut penalty = 0;

    // 预算感知：len 上限受剩余层预算约束（层序律的预算边界）：预算紧张时收缩到浅层，预算充足时才允许深层块占满预算
    let len_cap = max_len.min(budget.saturating_add(1));
    for n in (2..=len_cap).rev() {
        if layers.len() >= budget {
            break;
        }
        let scatter: BTreeSet<NonZero<Tick>> = (1..n)
            .map(|i| NonZero::new((i as Tick) * d).unwrap())
            .collect();
        let tec = BoundedTec::extract(&work, scatter).into_inner();
        if n == 2 {
            penalty = nested_penalty(&tec.kernel, d, &work);
        }
        let gain = tec.reuse();
        if gain <= 0 {
            continue;
        }
        total += gain;
        let expansion = tec.expand();
        layers.push(tec);
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
pub fn reuse_flow<E: Event>(
    source: &TePlane<E>,
    max_len: usize,
    max_layers: usize,
) -> (Vec<TransEqClass<E>>, usize, TePlane<E>) {
    let mut residual = source.clone();
    let mut plan: Vec<TransEqClass<E>> = Vec::new();
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

        let mut best: Option<((isize, usize, Reverse<Tick>), Vec<TransEqClass<E>>)> = None;
        for &d in &candidates {
            let budget = max_layers - plan.len();
            let (total, layers, penalty) = family_deep_first(&residual, d, max_len, budget);
            if total <= 0 {
                continue;
            }
            // 嵌套惩罚可能超过总量（Python 语义允许负 score，仅用于排序）
            let score = total as isize - penalty as isize;
            let deepest = layers.iter().map(|tec| tec.arity()).max().unwrap_or(0);
            let key = (score, deepest, Reverse(d));
            if best.as_ref().is_none_or(|(best_key, _)| key > *best_key) {
                best = Some((key, layers));
            }
        }
        let Some((_, layers)) = best else {
            break;
        };

        for tec in layers {
            total_reuse += tec.reuse();
            residual = subtract_exact(&residual, &tec.expand());
            plan.push(tec);
        }
    }

    (plan, total_reuse, residual)
}

/// Beam-search layer flow: each round explores up to `beam` candidate layer
/// sequences within the remaining budget, committing the globally best one.
///
/// Unlike the family-complete commit of [`reuse_flow`], layers from different
/// families compete freely at every step, so cross-family combinations emerge
/// naturally (e.g. a deep AP block from one family plus a pair from another).
pub fn reuse_flow_beam<E: Event>(
    source: &TePlane<E>,
    max_len: usize,
    max_layers: usize,
    beam: usize,
) -> (Vec<TransEqClass<E>>, usize, TePlane<E>) {
    #[derive(Clone)]
    struct Path<E: Event> {
        layers: Vec<(BTreeSet<NonZero<Tick>>, usize)>,
        work: TePlane<E>,
        acc: usize,
    }

    let mut residual = source.clone();
    let mut plan: Vec<TransEqClass<E>> = Vec::new();
    let mut total_reuse = 0;

    while plan.len() < max_layers {
        let budget = max_layers - plan.len();
        let support = autocorrelation(&residual);
        let Some(&max_support) = support.values().max() else {
            break;
        };
        if max_support == 0 {
            break;
        }

        let mut paths = vec![Path {
            layers: Vec::new(),
            work: residual.clone(),
            acc: 0,
        }];
        for _ in 0..budget {
            let mut next: Vec<Path<E>> = Vec::new();
            for path in &paths {
                // Enumerate candidate layers on this path's residual:
                // family-deep-first within each d, competing across families.
                let sup = autocorrelation(&path.work);
                let Some(&ms) = sup.values().max() else {
                    continue;
                };
                let threshold = ms / (max_len - 1);
                let mut cand: Vec<(BTreeSet<NonZero<Tick>>, usize)> = Vec::new();
                for (&d, &v) in &sup {
                    if v <= threshold {
                        continue;
                    }
                    for n in (2..=max_len).rev() {
                        let scatter: BTreeSet<NonZero<Tick>> = (1..n)
                            .map(|i| NonZero::new((i as Tick) * d).unwrap())
                            .collect();
                        let tec = BoundedTec::extract(&path.work, scatter).into_inner();
                        let gain = tec.reuse();
                        if gain == 0 {
                            continue;
                        }
                        cand.push((tec.scatter, gain));
                    }
                }
                cand.sort_by(|a, b| b.1.cmp(&a.1));
                for (scatter, gain) in cand.into_iter().take(beam) {
                    let tec = BoundedTec::extract(&path.work, scatter).into_inner();
                    let work = subtract_exact(&path.work, &tec.expand());
                    let mut layers = path.layers.clone();
                    layers.push((tec.scatter, gain));
                    next.push(Path {
                        layers,
                        work,
                        acc: path.acc + gain,
                    });
                }
            }
            next.sort_by(|a, b| b.acc.cmp(&a.acc));
            next.truncate(beam);
            paths = next;
            if paths.is_empty() {
                break;
            }
        }
        let Some(best) = paths.into_iter().next() else {
            break;
        };
        for (scatter, gain) in best.layers {
            let tec = BoundedTec::extract_from(&mut residual, scatter).into_inner();
            total_reuse += gain;
            plan.push(tec);
        }
    }

    (plan, total_reuse, residual)
}

/// Apply scatter rules in order, extracting a kernel per rule and subtracting
/// its expansion from the working set. Returns `(plan, total_reuse, residual)`.
pub fn manual_flow<E: Event>(
    source: &TePlane<E>,
    rules: &[Vec<Tick>],
) -> (Vec<TransEqClass<E>>, usize, TePlane<E>) {
    let mut residual = source.clone();
    let mut plan: Vec<TransEqClass<E>> = Vec::new();
    let mut total_reuse = 0;

    for rule in rules {
        let scatter: BTreeSet<NonZero<Tick>> = rule
            .iter()
            .copied()
            .filter(|&t| t != 0)
            .filter_map(NonZero::new)
            .collect();
        if scatter.is_empty() {
            continue;
        }
        let tec = BoundedTec::extract_from(&mut residual, scatter).into_inner();
        let gain = tec.reuse();
        if gain == 0 {
            continue;
        }
        total_reuse += gain;
        plan.push(tec);
    }

    (plan, total_reuse, residual)
}

// Layout adaptation
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Convert a reuse plan and residual into layout TECs.
///
/// TECs whose minimum offset gap is too tight for the tapped delay line
/// (repeater coarse >= 4, i.e. min gap >= 8) are skipped and absorbed back
/// into the residual (degenerate absorption).
///
/// Returns `(tecs, skipped_layers)` where the residual is appended as a
/// no-offset TEC when non-empty.
pub fn plan_to_tecs<E: Event>(
    plan: Vec<TransEqClass<E>>,
    mut residual: TePlane<E>,
) -> (Vec<TransEqClass<E>>, usize) {
    let mut tecs = Vec::new();
    let mut skipped = 0;
    for tec in plan {
        if tec.min_gap().is_none_or(|gap| gap < 8) {
            let expansion = tec.expand();
            for (event, count) in expansion.iter() {
                add_to(&mut residual, event.clone(), *count);
            }
            skipped += 1;
            continue;
        }
        tecs.push(tec);
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

    fn chain(n: usize, d: Tick, start: Tick) -> TePlane<Tone> {
        TePlane::from_iter((0..n).map(|i| (start + (i as Tick) * d, tone())))
    }

    /// Per-chain closed form: `f(T) = 3*floor(T/4) + [0,0,1,2][T mod 4]`.
    fn f(ticks: usize) -> usize {
        3 * (ticks / 4) + [0, 0, 1, 2][ticks % 4]
    }

    /// Composition check: `expand(plan) + residual == source`.
    fn verify_composition(
        source: &TePlane<Tone>,
        plan: &[TransEqClass<Tone>],
        residual: &TePlane<Tone>,
    ) {
        let mut total = residual.clone();
        for tec in plan {
            for (event, count) in tec.expand().iter() {
                add_to(&mut total, event.clone(), *count);
            }
        }
        assert_eq!(&total, source);
    }

    #[test]
    fn family_deep_first_matches_chain_closed_form() {
        for ticks in 2..16 {
            let (total, layers, _) = family_deep_first(&chain(ticks, 128, 1000), 128, 4, 3);
            assert_eq!(total, f(ticks), "T={ticks}");
            let lens: Vec<usize> = layers.iter().map(|tec| tec.arity()).collect();
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
        let m: TePlane<Tone> = TePlane::from_iter([
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
        let m: TePlane<Tone> =
            TePlane::from_iter([(0u32, t), (128u32, t), (4000u32, t), (4128u32, t)]);
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
            let m: TePlane<Tone> = TePlane::from_iter(ticks.iter().map(|&x| (x, t)));
            let (_, total, _) = reuse_flow(&m, 4, 1000);
            assert_eq!(total, *expected, "ticks={ticks:?}");
        }
    }
}
