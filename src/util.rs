use crate::note::{Note, Notes, Tone};
use crate::types::{Index, Position, Tick};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::{BTreeMap, BTreeSet};
use std::iter::repeat;
use std::num::NonZero;
use std::ops::{BitAnd, Deref, DerefMut};

// TpPlane
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A point in the TP (tick–tone) plane.
pub type Point = (Tick, Tone);

/// TP (tick–tone) plane multiset.
pub type TpPlane = Counter<Point>;

impl From<Notes> for TpPlane {
    fn from(notes: Notes) -> Self {
        notes
            .into_iter()
            .map(|(pos, note)| (pos.tick(), note.tone()))
            .collect()
    }
}

impl From<TpPlane> for Notes {
    fn from(plane: TpPlane) -> Self {
        plane
            .into_iter()
            .flat_map(|((tick, tone), count)| repeat((tick, tone)).take(count))
            .into_group_map()
            .into_iter()
            .collect()
    }
}

// Vector Table
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Vector table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorTable {
    table: BTreeMap<Tick, TpPlane>,
}

impl VectorTable {
    /// Construct from a [`TpPlane`] by enumerating all point pairs.
    ///
    /// `step` controls the granularity: only offsets that are multiples of `step`
    /// are included in the table. Use `step = 1` to include all offsets.
    pub fn from_plane(plane: &TpPlane, loop_len: Option<NonZero<Tick>>, step: Tick) -> Self {
        let groups = plane
            .iter()
            .map(|(&(t, tone), &c)| (tone, (t, c)))
            .into_group_map();
        let half = loop_len.map(|l| l.get() / 2);
        let mut table: BTreeMap<Tick, TpPlane> = BTreeMap::new();

        let normalize = |raw, left, right| match raw > half.unwrap_or(raw) {
            true => (right, loop_len.unwrap().get() - raw),
            false => (left, raw),
        };
        let mut insert = |off, point, mult| match off % step == 0 {
            true => table.entry(off).or_default().insert(point, mult),
            false => None,
        };

        for (tone, ticks) in groups {
            for [&(lt, lc), &(rt, rc)] in ticks.iter().sorted().array_combinations() {
                let (anchor, norm) = normalize(rt - lt, lt, rt);
                insert(norm, (anchor, tone), lc.min(rc));
            }
        }
        Self { table }
    }

    /// Mine the vector table for a balanced minimal [`TransEqClass`] using
    /// FP-Growth to discover frequent offset sets, then selecting the one
    /// with the best effective-coverage-to-simplicity ratio.
    ///
    /// `min_support` (0.0–1.0) controls the minimum relative frequency an
    /// offset must have among anchor points to be considered frequent.
    #[deprecated(note = "FP-Growth TEC mining is under analysis; results are unreliable")]
    pub fn mine_tec(&self, min_support: f64) -> Option<TransEqClass> {
        if self.len() < 2 {
            return None;
        }

        // Build transaction database:
        // For each unique anchor point, collect all offsets whose plane contains it.
        // Each such (point → set of offsets) is one transaction.
        let mut point_offsets: BTreeMap<Point, BTreeSet<Tick>> = BTreeMap::new();
        for (&offset, plane) in self.iter() {
            for (point, _count) in plane.iter() {
                point_offsets.entry(*point).or_default().insert(offset);
            }
        }

        let n_transactions = point_offsets.len();
        if n_transactions == 0 {
            return None;
        }

        let min_abs = (n_transactions as f64 * min_support.clamp(0.0, 1.0))
            .ceil()
            .max(1.0) as usize;
        let transactions: Vec<BTreeSet<Tick>> = point_offsets.into_values().collect();

        // Phase 1: Build FP-tree.
        let tree = FpTree::new(&transactions, min_abs)?;

        // Phase 2: Mine all frequent itemsets.
        let itemsets = tree.mine();

        // Phase 3: Score each itemset and return the best TEC.
        let mut best: Option<TransEqClass> = None;
        let mut best_score = 0.0f64;

        for itemset in &itemsets {
            if itemset.len() < 2 {
                continue;
            }

            let offsets: BTreeSet<NonZero<Tick>> =
                itemset.iter().filter_map(|&t| NonZero::new(t)).collect();
            if offsets.len() < 2 {
                continue;
            }

            // Intersect all offset planes to get the common anchor points.
            let first_offset = match offsets.first() {
                Some(o) => o,
                None => continue,
            };
            let Some(mut points) = self.get(&first_offset.get()).cloned() else {
                continue;
            };
            for offset in offsets.iter().skip(1) {
                let Some(plane) = self.get(&offset.get()).cloned() else {
                    continue;
                };
                points = points & plane;
            }

            if points.is_empty() {
                continue;
            }

            // Score: effective (pruned) points² / n_offsets.
            #[allow(deprecated)]
            let effective = TransEqClass {
                offsets: offsets.clone(),
                points: points.clone(),
            }
            .prune()
            .len();

            let score = (effective as f64).powi(2) / offsets.len() as f64;
            if score > best_score {
                best_score = score;
                best = Some(TransEqClass { offsets, points });
            }
        }

        best
    }

    /// Mine the largest TEC with at least `min_offsets` offsets using greedy
    /// plane intersection. Unlike `mine_tec` (which requires frequency via
    /// FP-Growth), this only needs non-empty intersection — it finds TECs
    /// with more offsets that FP-Growth might miss due to support threshold.
    #[deprecated(note = "greedy TEC mining is under analysis; results are unreliable")]
    pub fn find_largest_tec(&self, min_offsets: usize) -> Option<TransEqClass> {
        // Collect offsets sorted by plane size (most anchors first).
        let mut planes: Vec<(&Tick, &TpPlane)> = self.iter().collect();
        planes.sort_by(|(_, a), (_, b)| b.len().cmp(&a.len()));

        if planes.len() < min_offsets {
            return None;
        }

        let mut best: Option<TransEqClass> = None;
        let mut best_score = 0usize;

        for start in 0..planes.len() {
            let (&first_off, first_plane) = planes[start];
            let mut offsets = BTreeSet::from([NonZero::new(first_off).unwrap()]);
            let mut points = (*first_plane).clone();

            for &(&off, ref plane) in &planes[start + 1..] {
                let candidate = points.clone() & (**plane).clone();
                if !candidate.is_empty() {
                    if let Some(nz) = NonZero::new(off) {
                        offsets.insert(nz);
                        points = candidate;
                    }
                }
            }

            if offsets.len() >= min_offsets {
                #[allow(deprecated)]
                let effective = TransEqClass {
                    offsets: offsets.clone(),
                    points: points.clone(),
                }
                .prune()
                .len();

                // Score: favor more offsets, then more effective anchors.
                let score = offsets.len() * effective;
                if score > best_score {
                    best_score = score;
                    best = Some(TransEqClass { offsets, points });
                }
            }
        }

        best
    }
}

impl Deref for VectorTable {
    fn deref(&self) -> &Self::Target {
        &self.table
    }
    type Target = BTreeMap<Tick, TpPlane>;
}

impl DerefMut for VectorTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.table
    }
}

// Translation Equivalence Class
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Translation Equivalence Class (TEC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransEqClass {
    /// Offsets defining the translation pattern.
    offsets: BTreeSet<NonZero<Tick>>,
    /// Points (multiset) that this TEC operates on.
    points: TpPlane,
}

impl TransEqClass {
    /// The offsets that define this translation pattern.
    pub fn offsets(&self) -> &BTreeSet<NonZero<Tick>> {
        &self.offsets
    }

    /// The anchor points common to all offsets.
    pub fn points(&self) -> &TpPlane {
        &self.points
    }

    /// Conservatively reduce TEC to arithmetic kernel K, discarding conflicting points.
    #[deprecated(note = "TEC analysis is under analysis; prune will be redesigned")]
    pub fn prune(self) -> TpPlane {
        let mut points = self.points;
        let indexes: Vec<Point> = points.keys().copied().sorted().collect();

        for (point @ (tick, tone), scatter) in iproduct!(indexes, self.offsets.iter()) {
            let shifted = &(tick + scatter.get(), tone);
            let anchor_mult = points[&point];
            let entry = points.entry(*shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        points
    }

    /// Decompose [`Notes`] into two parts using the arithmetic kernel of this TEC:
    ///
    /// 1. Prune the raw anchor set to the minimal generating kernel.
    /// 2. Expand kernel + offsets to get the actual (tick, tone) positions.
    ///
    /// - **Pattern notes**: notes matching the kernel-expanded positions.
    /// - **Residual notes**: everything else.
    #[deprecated(note = "TEC analysis is under analysis; decompose will be redesigned")]
    pub fn decompose(&self, notes: &Notes, song_len: Tick) -> (Notes, Notes) {
        // Prune to arithmetic kernel: discard points that are images of others.
        let kernel = self.clone().prune();

        // Expand kernel + offsets to get all covered (tick, tone) positions.
        // Skip points with count == 0 (eliminated by prune).
        let mut pattern_set: BTreeSet<(Tick, Tone)> = BTreeSet::new();
        for (&(anchor, tone), count) in &kernel {
            if *count == 0 {
                continue;
            }
            pattern_set.insert((anchor, tone));
            for offset in &self.offsets {
                let shifted = anchor + offset.get();
                if shifted < song_len {
                    pattern_set.insert((shifted, tone));
                }
            }
        }

        let mut pattern: BTreeMap<Position, Note> = BTreeMap::new();
        let mut residual: BTreeMap<Position, Note> = BTreeMap::new();

        for (pos, note) in notes.iter() {
            if pattern_set.contains(&(pos.tick(), note.tone())) {
                pattern.insert(*pos, note.clone());
            } else {
                residual.insert(*pos, note.clone());
            }
        }

        (pattern.into(), residual.into())
    }
}

impl BitAnd for TransEqClass {
    fn bitand(self, rhs: Self) -> Self {
        let offsets = &self.offsets | &rhs.offsets;
        let points = self.points & rhs.points;
        Self { offsets, points }
    }
    type Output = Self;
}

impl<const N: usize> From<([NonZero<Tick>; N], TpPlane)> for TransEqClass {
    fn from((offsets, points): ([NonZero<Tick>; N], TpPlane)) -> Self {
        let offsets = BTreeSet::from(offsets);
        Self { offsets, points }
    }
}

// FP-Growth Miner
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Node in the FP-tree.
#[derive(Clone)]
#[deprecated(note = "FP-Growth analysis is under analysis; will be replaced")]
struct FpNode {
    /// The offset value this node represents.
    item: Tick,
    /// Number of transactions passing through this node.
    count: usize,
    /// Index of the parent node.
    parent: usize,
    /// Indices of child nodes.
    children: Vec<usize>,
    /// Next node in the same-item linked list (header table chain).
    next: Option<usize>,
}

/// FP-tree for frequent pattern mining using the FP-Growth algorithm.
#[deprecated(note = "FP-Growth analysis is under analysis; will be replaced")]
struct FpTree {
    nodes: Vec<FpNode>,
    /// header table: item → (total_count, first_node_index)
    header: BTreeMap<Tick, (usize, Option<usize>)>,
    min_support: usize,
}

impl FpTree {
    /// Build an FP-tree from transactions.
    fn new(transactions: &[BTreeSet<Tick>], min_support: usize) -> Option<Self> {
        // First pass: count item frequencies across all transactions.
        let mut freq: BTreeMap<Tick, usize> = BTreeMap::new();
        for txn in transactions {
            for &item in txn {
                *freq.entry(item).or_insert(0) += 1;
            }
        }

        // Filter by min_support, sort by frequency descending.
        let mut freq_items: Vec<(Tick, usize)> = freq
            .into_iter()
            .filter(|&(_, count)| count >= min_support)
            .collect();
        if freq_items.is_empty() {
            return None;
        }
        freq_items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Build header table: item → (total_count, first_node_index).
        let mut header: BTreeMap<Tick, (usize, Option<usize>)> = BTreeMap::new();
        for &(item, count) in &freq_items {
            header.insert(item, (count, None));
        }

        // Item priority: lower index = higher frequency.
        let item_priority: BTreeMap<Tick, usize> = freq_items
            .iter()
            .enumerate()
            .map(|(i, &(item, _))| (item, i))
            .collect();

        // Build the FP-tree (root = index 0).
        let mut nodes = vec![FpNode {
            item: Tick::MAX,
            count: 0,
            parent: 0,
            children: vec![],
            next: None,
        }];

        for txn in transactions {
            // Keep only frequent items, sort by frequency descending.
            let mut items: Vec<Tick> = txn
                .iter()
                .filter(|item| item_priority.contains_key(item))
                .copied()
                .collect();
            items.sort_by(|a, b| item_priority[a].cmp(&item_priority[b]));

            if items.is_empty() {
                continue;
            }

            let mut current = 0;
            for &item in &items {
                // Check if a child with this item already exists.
                let child = nodes[current]
                    .children
                    .iter()
                    .find(|&&child_idx| nodes[child_idx].item == item)
                    .copied();

                if let Some(child_idx) = child {
                    nodes[child_idx].count += 1;
                    current = child_idx;
                } else {
                    let new_idx = nodes.len();
                    nodes.push(FpNode {
                        item,
                        count: 1,
                        parent: current,
                        children: vec![],
                        next: None,
                    });
                    nodes[current].children.push(new_idx);

                    // Link into header table chain (prepend).
                    let (_, first) = header.get_mut(&item).unwrap();
                    nodes[new_idx].next = *first;
                    *first = Some(new_idx);

                    current = new_idx;
                }
            }
        }

        Some(Self {
            nodes,
            header,
            min_support,
        })
    }

    /// Mine all frequent itemsets.
    fn mine(&self) -> Vec<Vec<Tick>> {
        let mut result = Vec::new();
        let mut prefix = Vec::new();
        self.grow(&mut prefix, &mut result);
        result
    }

    /// Recursive FP-Growth: process items in ascending frequency order.
    fn grow(&self, prefix: &mut Vec<Tick>, result: &mut Vec<Vec<Tick>>) {
        // Collect items sorted by frequency ascending.
        let mut items: Vec<(Tick, usize)> = self
            .header
            .iter()
            .map(|(&item, &(count, _))| (item, count))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        for &(item, _) in &items {
            // Extend pattern with this item.
            prefix.push(item);
            result.push(prefix.clone());

            // Build conditional FP-tree and recurse.
            if let Some(cond_tree) = self.build_conditional_tree(item) {
                cond_tree.grow(prefix, result);
            }

            prefix.pop();
        }
    }

    /// Build a conditional FP-tree for a given item.
    fn build_conditional_tree(&self, item: Tick) -> Option<Self> {
        // Collect conditional pattern base:
        // all paths from root to nodes containing `item`.
        let mut prefix_paths: Vec<(Vec<Tick>, usize)> = Vec::new();
        let mut next = self.header.get(&item).and_then(|(_, first)| *first);

        while let Some(node_idx) = next {
            let node = &self.nodes[node_idx];
            let count = node.count;

            // Build the prefix path (root → parent of this node).
            let mut path = Vec::new();
            let mut curr = node.parent;
            while curr != 0 {
                path.push(self.nodes[curr].item);
                curr = self.nodes[curr].parent;
            }
            path.reverse();

            if !path.is_empty() {
                prefix_paths.push((path, count));
            }

            next = node.next;
        }

        if prefix_paths.is_empty() {
            return None;
        }

        // Build conditional transactions from prefix paths.
        let mut cond_txns: Vec<BTreeSet<Tick>> = Vec::new();
        for (path, count) in prefix_paths {
            let set: BTreeSet<Tick> = path.into_iter().collect();
            cond_txns.extend(std::iter::repeat(set).take(count));
        }

        Self::new(&cond_txns, self.min_support)
    }
}

// Notes util
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Notes {
    /// Rescales ticks from arbitrary tempo (tick/s) to standard game tick (20 t/s).
    pub fn rescale_to_game_tick(self, tempo: f32) -> Notes {
        self.rescale_to_tick_rate(tempo, 20)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to redstone tick (10 t/s).
    pub fn rescale_to_redstone_tick(self, tempo: f32) -> Notes {
        self.rescale_to_tick_rate(tempo, 10)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to the given target tick rate (t/s).
    pub fn rescale_to_tick_rate(self, tempo: f32, target_rate: u32) -> Notes {
        let scale = (target_rate as f32 / tempo).round() as u32;
        let map_pos = |pos: Position| Position::new(pos.tick() * scale, pos.layer());
        match scale > 1 {
            true => self.into_iter().map(|(p, n)| (map_pos(p), n)).collect(),
            false => self,
        }
    }

    /// Groups notes into contiguous blocks separated by empty layers.
    pub fn split_by_layer_gaps(self) -> Vec<Notes> {
        let layers: BTreeSet<Index> = self.keys().map(|pos| pos.layer()).collect();
        let block_start = |prev: &mut Option<Index>, curr: Index| {
            let keep = prev.map_or(true, |p| p + 1 != curr);
            *prev = Some(curr);
            Some(keep.then_some(curr))
        };
        let starts: Vec<Index> = layers
            .into_iter()
            .scan(None, block_start)
            .flatten()
            .collect();

        let mut groups: Vec<Notes> = vec![Default::default(); starts.len()];
        for (pos, note) in self {
            let idx = starts.partition_point(|&s| s <= pos.layer()) - 1;
            let pos = Position::new(pos.tick(), pos.layer() - starts[idx]);
            groups[idx].insert(pos, note);
        }
        groups
    }

    /// Splits notes into groups of `size` layers each.
    pub fn split_by_layer_count(self, size: Option<NonZero<usize>>) -> Vec<Notes> {
        let Some(size) = size else {
            return vec![self];
        };
        let size = size.get();
        let mut groups: BTreeMap<Index, BTreeMap<Position, Note>> = BTreeMap::new();
        for (pos, note) in self {
            let group = pos.layer() / size as Index;
            let new_layer = pos.layer() % size as Index;
            let entry = groups.entry(group).or_default();
            entry.insert(Position::new(pos.tick(), new_layer), note);
        }
        groups.into_values().map(Notes::from).collect()
    }

    /// separates notes into matched and unmatched groups via pattern matching.
    pub fn matches_by<F>(self, pattern: &[Tick], song_length: Tick, f: F) -> (Notes, Notes)
    where
        F: Fn(&Note, &Note) -> bool,
    {
        struct NoteWithMatch {
            pos: Position,
            note: Note,
            is_matched: bool,
        }
        let mut candidates: Vec<NoteWithMatch> = self
            .into_iter()
            .map(|(pos, note)| NoteWithMatch {
                pos,
                note,
                is_matched: false,
            })
            .collect();

        for i in 0..candidates.len() {
            if candidates[i].is_matched {
                continue;
            }

            let base = candidates[i].pos.tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, p)| {
                    !p.is_matched && p.pos.tick() == target && f(&p.note, &candidates[i].note)
                });
                found.map(|(idx, _)| {
                    indices.push(idx);
                    indices
                })
            });

            if let Some(indices) = result {
                for &idx in &indices {
                    candidates[idx].is_matched = true;
                }
            }
        }

        let (mut matched, mut unmatched) = (BTreeMap::new(), BTreeMap::new());
        for note in candidates {
            match note.is_matched {
                true => matched.insert(note.pos, note.note),
                false => unmatched.insert(note.pos, note.note),
            };
        }
        (matched.into(), unmatched.into())
    }

    /// like matches_by but preserves group boundaries, returns MatchedGroups.
    #[allow(deprecated)]
    pub fn group_match<F>(self, pattern: &[Tick], song_length: Tick, f: F) -> (MatchedGroups, Notes)
    where
        F: Fn(&Note, &Note) -> bool,
    {
        struct Candidate {
            pos: Position,
            note: Note,
            is_matched: bool,
            group: usize,
        }

        let mut candidates: Vec<Candidate> = self
            .into_iter()
            .map(|(pos, note)| Candidate {
                pos,
                note,
                is_matched: false,
                group: 0,
            })
            .collect();

        let mut group_cnt = 0;
        for i in 0..candidates.len() {
            if candidates[i].is_matched {
                continue;
            }

            let base = candidates[i].pos.tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, c)| {
                    !c.is_matched && c.pos.tick() == target && f(&c.note, &candidates[i].note)
                });
                found.map(|(idx, _)| {
                    indices.push(idx);
                    indices
                })
            });

            if let Some(indices) = result {
                for &idx in &indices {
                    candidates[idx].is_matched = true;
                    candidates[idx].group = group_cnt;
                }
                group_cnt += 1;
            }
        }

        let mut groups: Vec<BTreeMap<Position, Note>> =
            (0..group_cnt).map(|_| BTreeMap::new()).collect();
        let mut unmatched = BTreeMap::new();
        for c in candidates {
            if c.is_matched {
                groups[c.group].insert(c.pos, c.note);
            } else {
                unmatched.insert(c.pos, c.note);
            }
        }

        (
            MatchedGroups {
                groups: groups.into_iter().map(Into::into).collect(),
            },
            unmatched.into(),
        )
    }

    /// Concatenate multiple note groups with blank layer separators.
    pub fn concat<'a>(notes: impl IntoIterator<Item = &'a Notes>) -> Self {
        let shift = |(pos, note): (&Position, &Note), base: Index| {
            (Position::new(pos.tick(), pos.layer() + base), note.clone())
        };
        let mut offset = 0;
        let stacked = notes.into_iter().flat_map(|n| {
            let base = offset.clone();
            offset += n.keys().map(|p| p.layer()).max().map_or(0, |m| m + 2);
            n.iter().map(move |pair| shift(pair, base))
        });
        stacked.collect()
    }

    /// reassign layers across multiple note groups so they don't overlap.
    #[deprecated(note = "use Notes::concat instead")]
    pub fn reassign_layers<I, J>(slices: I, gap: Index) -> Self
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = (Tick, Note)>,
    {
        let mut base_layer: Index = 0;
        let mut result: BTreeMap<Position, Note> = Default::default();

        for notes in slices {
            let mut prev_tick: Tick = Tick::MAX;
            let mut prev_layer: Index = Default::default();
            let mut layers: Index = 0;

            for (tick, note) in notes.into_iter().sorted_unstable() {
                prev_layer = if tick == prev_tick { prev_layer + 1 } else { 0 };
                layers = layers.max(prev_layer + 1 + gap);
                prev_tick = tick;
                result.insert(Position::new(tick, base_layer + prev_layer), note);
            }
            base_layer += layers;
        }

        result.into()
    }
}

/// pattern match result with group boundaries preserved.
/// each group corresponds to one complete pattern match.
#[deprecated(note = "this type is planned for deprecation")]
#[allow(deprecated)]
#[derive(Debug, Clone)]
pub struct MatchedGroups {
    groups: Vec<Notes>,
}

#[allow(deprecated)]
impl MatchedGroups {
    pub fn empty() -> Self {
        Self { groups: vec![] }
    }

    /// all matched groups, each group is all notes from one pattern match.
    pub fn groups(&self) -> &[Notes] {
        &self.groups
    }

    /// total number of matched notes.
    pub fn matched_len(&self) -> usize {
        self.groups.iter().map(|g| g.len()).sum()
    }

    /// template notes: first note of each group (base). for projection, one note per group.
    /// bases at different layers on the same tick are each preserved.
    pub fn templates(&self) -> Notes {
        self.groups
            .iter()
            .filter_map(|group| group.iter().next().map(|(p, n)| (*p, n.clone())))
            .collect()
    }
}
