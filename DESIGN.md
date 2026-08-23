# 红乐的形式化约简

> 该文件禁止 AI 编辑。文本未完成，内容注意辨别。

在游戏 Minecraft 中，由红石电路控制音符盒进行演奏，即通常所说的「红石音乐」。红石音乐可系统地分为控制电路与演奏单元两部分，二者相对独立又相互耦合。在一轮演奏周期中，同一演奏单元被多次激活的现象称为重复，这是利用电路约简红石音乐的核心逻辑。

体积描述的是表达式的实际结构规模，将 `5040` 写作 `7!` 是一种形式上的约简，形式化约简与体积不呈正相关。而控制电路与演奏单元的复杂度呈负相关，控制电路的退化过程不会使结构消失，而是会使结构转化为演奏单元的内部模式，约简的极限受乐谱本身形式上的不规则程度所约束。

## 基本框架

红石音乐可形式化为乐谱平面上的二维多重集 `M`：`T` 轴为时间刻度，`E` 轴为音效事件的枚举。在不引入微时序的前提下，红石电路在同一步长至多触发一次，故任意点的重数 `n` 在时间维度上不可分，而表示该音的加重程度。

本文形式化约简的核心可概括为等式 `M = K (+) S + R`，其中：

- `K` 为模板多重集，是 `M` 的子集，对应一组演奏单元。
- `S` 为 `K` 在 `T` 轴上偏移量的集合。
- `K (+) S` 为 `K` 与 `S` 的闵可夫斯基和 `{k + s | k ∈ K, s ∈ S}`，即模板经平移后的叠加。
- `R` 为残差多重集，即 `M` 中未被 `K (+) S` 覆盖的部分，也可递归地重复分解过程。

作为先验条件，`E` 轴各音效彼此独立，不存在变换关系。因此，平移仅沿 `T` 轴进行，基于平移的稀疏一维分析方法可同样适用于乐谱平面，下文均以一维多重集为简化的实验模型展开验证。

## 平移和分析

结构的重复可视为平移的叠加；平移不改变结构，因而重复结构可通过平移提取。若非零 `t` 使 `M` 与 `M+t` 相交，即表明 `M` 存在自相似性，此时可提取模板 `K` 与偏移集 `S`，以 `K (+) S` 压缩表达 `M` 的主体结构。

点对枚举是平移取交的具体实现，用于高效建立表示单步平移关系的向量表，即平移等价类 (Translation Equivalence Class)[^1]。其形式是将 `M²` 中的点对按偏移 `y − x` 划分为等价类，偏移 `t` 对应的等价类即 `{(x, x + t) | x ∈ M ∩ (M - t)}`，即证明点对枚举作为平移取交的高效实现行为等价。

平移取交是几何空间中的逻辑关系，而非算术变换。在空间中，一个点与所有点的距离关系同时成立、互不排斥。因此，同一点必然同时满足多个不同的偏移关系，逻辑上成立的关系即为算术上可实现关系的上界。

## 结构的分解

多重集的叠加 `K (+) S` 实质是各偏移位置的重数求和，因此要从多重集中拆解重复结构，需按依赖关系逐步扣除。平移取交 (即点对枚举) 将平移关系具体化为偏移对应的点集，表示平移的逻辑关系；逐步扣除在此基础上分配重数，最终得到反卷积的解，表示平移的算术关系。

在精确分解 (`R = 0`) 下，给定 `M` 与 `S`，逐步扣除即可解出 `K`：`K(t) = min[M(t + s) | s ∈ S]` (`t ∈ T`)，各 `M(t + s)` 同步扣除 `K(t)` 以更新状态。

当 `R ≠ 0` 时，解的唯一性不再成立。残差 `R` 沿扫描方向传播，不同方向对应残差的不同线性组合，故不同求解顺序将产生不同的 `K`。求解方向对残差分布施加隐式约束，使反卷积呈病态。

两个 `K` 点的覆盖集相交当且仅当两点间距等于 `S` 中某对偏移之差，当相交处的重数不足以分配时则为冲突。冲突表现为结果的歧义，即不同组合对应不同的 `K` 与残差分布。这种歧义下，可能存在多个组合同时达到全局最小残差，它们均为最优解。

作为经验，乐谱在覆盖集相交位置的加重变化较为少见，重数争抢导致的冲突鲜有发生。因此，通过直接修剪逻辑关系以满足算术约束，由此所得的 `K` 精度通常可用。

相对于有方向的逐步扣除，另一种求解质量更好的方法是基于冲突分配思想的：找出 `M` 中所有可能放置 `S` 的位置，每次选取冲突最小的组进行放置并扣除，重复这一过程。对于普通乐曲，本文认为有向逐步方法的精度已足够，故不对此方法展开讨论。

## 附录

基于逐步方法的反卷积实现：

```python
# 有状态逐步反卷积
def resolve(mset: list, scatter: set) -> list:
    length = len(mset)
    kernel = []
    for tick in range(length):
        scope = [tick + s for s in scatter if 0 <= tick + s < length]
        base = min(mset[i] for i in scope)
        for i in scope:
            mset[i] -= base
        kernel.append(base)
    return kernel

# example
mset = [0, 1, 2, 2, 1, 1, 1, 1]
scatter = {0, 1, 4}
print(f"M={mset}, S={scatter}, K={resolve(mset, scatter)}, R={mset}")
```

基于点对枚举的反卷积实现：

```python
from collections import Counter
from itertools import combinations, product

# 通过点对枚举构建向量表 (Translation Equivalence Class)
def pair_enum(mset: Counter) -> dict:
    pe = {}
    for (left, lc), (right, _rc) in combinations(sorted(mset.items()), 2):
        pe.setdefault(right - left, {})[left] = lc
    return pe

# 根据模式构建平移锚点
def anchor(pe: dict, scatter: set) -> Counter:
    dicts = [pe.get(s, {}) for s in scatter - {0}]
    anchors = set.intersection(*map(set, dicts))
    return Counter({a: min(d[a] for d in dicts) for a in anchors})

# 有损地修剪逻辑关系以满足算术约束，从锚点直接构建模板 K
def prune(anchors: Counter, scatter: set) -> Counter:
    for tick, s in product(sorted(anchors), scatter - {0}):
        anchors[tick + s] -= anchors[tick]
    return +anchors

# example
mset = Counter([1, 2, 3, 4, 5, 7, 2, 3, 6])
scatter = {0, 1, 4}
print(f"S={scatter}, K={prune(anchor(pair_enum(mset), scatter), scatter)}")
```

## 参考文献

[^1]: Meredith D, Lemström K, Wiggins G A. Algorithms for discovering repeated patterns in multidimensional representations of polyphonic music[C]. Cambridge Music Processing Colloquium 2003, Department of Engineering, University of Cambridge. 2003.
