# Optimal Splitting Algorithm

This document describes how to split coins in Coinage before executing a payment so that the number of newly generated coins is minimal. The algorithm is efficient: it runs in linear time. We then compare it with the algorithm currently implemented in Polkadot Mobile and show that the latter is not optimal.

## Introduction

Recall that:

- every coin in Coinage has a **denomination** $k$ with $0 \leq k \leq K$, where the **maximal denomination** $K$ is a global constant, and its value is $2^k$ (in cents);
- a coin that has not reached the maximal age can be transferred, or split into at most $M$ new coins; **throughout we assume $M > K$**;
- a **split** of a coin of denomination $k$ consumes that coin and creates a sequence of coins of denominations $(k_1, \ldots, k_l)$ with $l \leq M$ that conserves the total value, that is, $2^{k_1} + \cdots + 2^{k_l} = 2^k$.

We represent the coins a user owns by a **count vector** $\mathbf{n} = (n_0, \ldots, n_K)$, where $n_k$ is the number of coins of denomination $k$. Only coins below the maximal age are counted, since coins at the maximal age can be neither transferred nor split. The total value the user owns is

$$T \;=\; n_0 \cdot 2^0 + \cdots + n_K \cdot 2^K .$$

Throughout, the user wishes to send an amount $A \leq T$ to another user. This is possible directly only if some selection of coins sums to $A$, that is, if there are $a_k \leq n_k$ with $a_02^0 + \cdots + a_K 2^K = A$. In that case we say that **$A$ is reachable from $\mathbf{n}$**.

Otherwise the user must first split coins. This may take an arbitrary number of split operations, and in full generality they may even be sequential: an output of one split can serve as the input of another. We call a sequence of splits after which $A$ is reachable from the resulting coins a **plan**.

For a plan $P$ we write $c(P)$ for the number of newly created coins: if $P$ consists of splits $s_1, \ldots, s_j$ and split $s_i$ produces $o_i$ output coins while consuming one, then

$$c(P) \;=\; \sum_{i=1}^{j} (o_i - 1).$$

Our goal is an efficient algorithm that finds a plan $P$ minimizing $c(P)$.

## Splitting trees

We first build some intuition about splits.

Consider a single split of a coin of denomination $k$ with output denominations $(k_1, \ldots, k_l)$. Because splits preserve total value, such a split can always be represented as a *full* binary tree (one in which every node has either two children or none) whose root carries the input coin and in which each output coin $k_i$ corresponds to exactly one leaf at depth $k - k_i$.

*Proof.* We argue by induction on $l$. If $l = 1$ then $k_1 = k$ and the tree consists of the root alone.

Let $l \geq 2$ and let $k^\star$ be the smallest output denomination. Since there is more than one output, $k^\star < k$. Suppose no second output had denomination $k^\star$. Then every other output coin, as well as the input coin, would have a value divisible by $2^{k^\star + 1}$, so that

$$0 \;=\; 2^k \bmod 2^{k^\star+1} \;=\; \Big(\sum\nolimits_{i=1}^{l} 2^{k_i}\Big) \bmod 2^{k^\star+1} \;=\; 2^{k^\star},$$

the residue contributed by the single coin of denomination $k^\star$ (a contradiction). Hence at least two outputs have denomination $k^\star$.

Now form a new split from the given one by replacing two outputs of denomination $k^\star$ with a single output of denomination $k^\star + 1$. This conserves value, and $k^\star + 1 \leq k \leq K$, so it is a valid split of the same input coin, with $l - 1$ outputs. By the induction hypothesis it is the leaf set of a full binary tree in which the merged coin sits at depth $k - (k^\star + 1) = k - k^\star - 1$. Splitting that leaf into two children at depth $k - k^\star$, carrying the two original coins, yields the desired tree. $\blacksquare$

We call this representation the **splitting tree** of the split. Its height is $k - k^\star$, since the smallest output denomination gives the deepest leaf.

A full binary tree of height $h$ has at least $h + 1$ leaves. Consequently, if $k^\star$ is the smallest output denomination of a split of a coin of denomination $k$, that split consumes one coin and produces at least $k - k^\star + 1$ coins.

## Lower bound

For a count vector $\mathbf{m} = (m_0, \ldots, m_K)$ and $1 \leq j \leq K + 1$ we write

$$\Sigma_{<j}(\mathbf{m}) \;=\; \sum_{k=0}^{j-1} m_k 2^k$$

for the total value of all coins of denomination below $j$.

Call an index $j$ with $1 \leq j \leq K$ **forced** if $A \bmod 2^j > \Sigma_{<j}(\mathbf{n})$. Whether $j$ is forced depends only on $A$ and $\mathbf{n}$, never on the plan. Let $F$ be the set of all forced indices.

**Theorem A.** *Every plan $P$ satisfies $c(P) \geq |F|$.*

*Proof.* Let $\mathbf{m} = (m_0, \ldots, m_K)$ be the count vector resulting from $P$. Since splits preserve total value, $\sum_{k=0}^{K} n_k 2^k = \sum_{k=0}^{K} m_k 2^k$.

Fix $j \in F$. As $P$ is a plan, $A$ is reachable from $\mathbf{m}$: there are $a_k \leq m_k$ with $\sum_{k=0}^{K} a_k 2^k = A$. For $k \geq j$ we have $2^k \bmod 2^j = 0$, hence $\big(\sum_{k=j}^{K} a_k 2^k\big) \bmod 2^j = 0$, and therefore

$$A \bmod 2^j \;\leq\; \sum_{k=0}^{j-1} a_k 2^k .$$

This uses the elementary fact that $a \bmod c = 0$ implies $(a + b) \bmod c \leq b$ for all $a, b \geq 0$ and $c > 0$; here $a = \sum_{k=j}^{K} a_k 2^k$, $b = \sum_{k=0}^{j-1} a_k 2^k$, and $c = 2^j$. Since $a_k \leq m_k$, it follows that $\Sigma_{<j}(\mathbf{m}) \geq \sum_{k=0}^{j-1} a_k 2^k \geq A \bmod 2^j$, and as $j$ is forced,

$$\Sigma_{<j}(\mathbf{m}) \;>\; \Sigma_{<j}(\mathbf{n}).$$

So this quantity is strictly larger after $P$ than before, and at least one split $s$ of $P$ must have increased it. Let $k_s$ be the input denomination of $s$ and $b_s$ its smallest output denomination. Because splits preserve total value, an increase is possible only if the input is not counted while at least one output is, that is, if $b_s < j \leq k_s$. Write $I_s = (b_s, k_s]$ for this half-open interval, so that $j \in I_s$.

Choosing one such split for each $j \in F$ yields *distinct* splits $s_1, \ldots, s_q$ of $P$ with $F \subseteq I_{s_1} \cup \cdots \cup I_{s_q}$. We showed above that a split $s$ consumes one coin and produces at least $k_s - b_s + 1$, so it increases the number of coins by at least $k_s - b_s = |I_s|$. Since every split contributes nonnegatively to $c(P)$,

$$c(P) \;\geq\; |I_{s_1}| + \cdots + |I_{s_q}| \;\geq\; \big|I_{s_1} \cup \cdots \cup I_{s_q}\big| \;\geq\; |F| . \qquad \blacksquare$$

## Attaining the lower bound

We now show that a plan $P$ with $c(P) = |F|$ can always be constructed efficiently. The construction is what one obtains by forcing equality in every inequality of the proof of Theorem A.

Partition $F$ into its maximal intervals $I_1, \ldots, I_q$ (the unique family of pairwise nonoverlapping, nonadjacent intervals whose union is $F$) and write $I_i = (t_i, k_i]$.

A **comb** of height $h$ is the maximally unbalanced full binary tree of height $h$: it has two leaves at depth $h$ and one leaf at each depth $1 \leq d < h$, so $h + 1$ leaves in total.

**Theorem B.** *For each $1 \leq i \leq q$ the count vector $\mathbf{n}$ contains a coin of denomination $k_i$. Splitting one such coin for every $i$, with a splitting tree that is a comb of height $k_i - t_i$, yields a valid plan $P$ with $c(P) = |F|$.*

*Proof.* **Existence of the coins.** By the definition of the maximal intervals, $k_i \in F$ while $k_i + 1 \notin F$. From $k_i \in F$,

$$A \bmod 2^{k_i+1} \;\geq\; A \bmod 2^{k_i} \;>\; \Sigma_{<k_i}(\mathbf{n}).$$

If $k_i = K$, then $\sum_{k=0}^{k_i} n_k 2^k = \sum_{k=0}^{K} n_k 2^k = T \geq A \geq A \bmod 2^{k_i+1}$ by assumption. If $k_i < K$, then $k_i + 1 \notin F$ gives $\sum_{k=0}^{k_i} n_k 2^k = \Sigma_{<k_i+1}(\mathbf{n}) \geq A \bmod 2^{k_i+1}$. Either way,

$$\sum_{k=0}^{k_i} n_k 2^k \;\geq\; A \bmod 2^{k_i+1} \;>\; \sum_{k=0}^{k_i-1} n_k 2^k ,$$

so $n_{k_i} 2^{k_i} > 0$ and therefore $n_{k_i} \geq 1$.

**$P$ is a valid plan with $c(P) = |F|$.** The intervals are pairwise disjoint, so the denominations $k_1, \ldots, k_q$ are distinct and the splits consume distinct coins; they are independent of one another and may be performed in any order, or in parallel. A comb of height $k_i - t_i$ has $k_i - t_i + 1 \leq K + 1 \leq M$ leaves, so each split is legal. Each consumes one coin and produces $k_i - t_i + 1$, whence

$$c(P) \;=\; \sum_{i=1}^{q} (k_i - t_i) \;=\; \sum_{i=1}^{q} |I_i| \;=\; |F| .$$

Let $\mathbf{m} = (m_0, \ldots, m_K)$ be the resulting count vector. It remains to prove that $A$ is reachable from $\mathbf{m}$.

**An auxiliary bound.** We first show that

$$A \bmod 2^j \;\leq\; \Sigma_{<j}(\mathbf{m}) \qquad \text{for every } 1 \leq j \leq K+1 . \tag{$\ast$}$$

For $j = K+1$ this is immediate: splits preserve total value, so $\Sigma_{<K+1}(\mathbf{m}) = \Sigma_{<K+1}(\mathbf{n}) = T \geq A$.

Let $1 \leq j \leq K$. A split leaves $\Sigma_{<j}$ unchanged whenever its input and all its outputs have denomination $\geq j$, or all have denomination $< j$, again because value is preserved. For our splits this is the case for every interval other than one containing $j$: an interval lying above $j$ has $t_{i'} \geq k_i \geq j$, and one lying below has $k_{i'} \leq t_i < j$.

If $j \in F$, let $I_i$ be the unique interval containing it. The difference $\Sigma_{<j}(\mathbf{m}) - \Sigma_{<j}(\mathbf{n})$ is then exactly the total value of those outputs of the $i$-th split whose denomination is below $j$ (namely two coins of denomination $t_i$ and one coin of denomination $d$ for each $t_i < d < j$). Hence

$$\Sigma_{<j}(\mathbf{m}) - \Sigma_{<j}(\mathbf{n}) \;=\; 2 \cdot 2^{t_i} + \sum_{d = t_i+1}^{j-1} 2^d \;=\; 2^j ,$$

so $\Sigma_{<j}(\mathbf{m}) \geq 2^j > A \bmod 2^j$ and $(\ast)$ holds.

If $j \notin F$, then no interval contains $j$, so $\Sigma_{<j}(\mathbf{m}) = \Sigma_{<j}(\mathbf{n})$, and $(\ast)$ is precisely the definition of $j$ not being forced.

**Reachability.** We prove by induction on $0 \leq j \leq K+1$: for every $x \geq 0$ with $0 \leq A - x 2^j \leq \Sigma_{<j}(\mathbf{m})$, the value $A - x 2^j$ is reachable from $\mathbf{m}$ using only coins of denomination $< j$.

For $j = 0$ we have $\Sigma_{<0}(\mathbf{m}) = 0$, so $A - x 2^0 = 0$, which is reachable by the empty selection.

Assume the claim for some $0 \leq j \leq K$ and let $x$ satisfy $0 \leq A - x 2^{j+1} \leq \Sigma_{<j+1}(\mathbf{m})$. Choose $a_j$ maximal with $a_j \leq m_j$ and $a_j 2^j \leq A - x 2^{j+1}$, and put $x' = 2x + a_j$, so that $A - x' 2^j = A - x 2^{j+1} - a_j 2^j$. We verify the induction hypothesis for $x'$.

That $A - x' 2^j \geq 0$ is immediate from the choice of $a_j$. Suppose, for contradiction, that $A - x' 2^j > \Sigma_{<j}(\mathbf{m})$. By $(\ast)$ we have $A \bmod 2^j \leq \Sigma_{<j}(\mathbf{m}) < A - x' 2^j$, and since $A - x' 2^j \equiv A \pmod{2^j}$ this forces $A - x' 2^j \geq 2^j$. Hence $A - x 2^{j+1} - a_j 2^j - 2^j \geq 0$, that is, $(a_j + 1) 2^j \leq A - x 2^{j+1}$; by maximality of $a_j$ this is possible only if $a_j = m_j$. But then

$$A - x' 2^j \;=\; A - x 2^{j+1} - m_j 2^j \;\leq\; \Sigma_{<j+1}(\mathbf{m}) - m_j 2^j \;=\; \Sigma_{<j}(\mathbf{m}),$$

contradicting the assumption. So $0 \leq A - x' 2^j \leq \Sigma_{<j}(\mathbf{m})$.

By the induction hypothesis $A - x' 2^j$ is reachable from $\mathbf{m}$ using only coins of denomination $< j$. Adding $a_j \leq m_j$ coins of denomination $j$ (none of which has been used) shows that $A - x' 2^j + a_j 2^j = A - x 2^{j+1}$ is reachable using only coins of denomination $< j+1$, completing the induction.

Applying the claim with $j = K+1$ and $x = 0$, and noting $0 \leq A \leq T = \Sigma_{<K+1}(\mathbf{m})$, we conclude that $A$ is reachable from $\mathbf{m}$. $\blacksquare$

**Corollary.** *The plan of Theorem B is optimal: it attains $\min_P c(P) = |F|$.*

*Proof.* Theorem A gives $c(P) \geq |F|$ for every plan, and Theorem B exhibits a plan attaining this value. $\blacksquare$

## The algorithm

**Algorithm OPTIMAL-PLAN**

- **Input:** $A$ and $\mathbf{n} = (n_0, \ldots, n_K)$ with $A \leq \sum_{k=0}^{K} n_k 2^k$.
- **Output:** a plan $P$ creating the minimum number of new coins.
- **Steps:**
  - $F \leftarrow$ empty list; $p \leftarrow 0$; $P \leftarrow$ empty plan
  - for $j$ from $1$ to $K$:
    - $p \leftarrow p + n_{j-1} 2^{j-1}$  &nbsp; (so that $p = \Sigma_{<j}(\mathbf{n})$)
    - if $A \bmod 2^j > p$: append $j$ to $F$
  - for each maximal interval $(t, k]$ of $F$:
    - add to $P$ the split with
      - input: one coin of denomination $k$
      - output: two coins of denomination $t$, and one of each denomination $t+1, \ldots, k-1$
  - return $P$

Given the count vector, the algorithm runs in $\mathcal{O}(K)$ time; building the count vector from a wallet of $N$ coins costs a further $\mathcal{O}(N)$.

The number of splits it generates is the number of maximal intervals of $F$. This is largest when the intervals are singletons separated by single gaps, so there are at most $\lceil K/2 \rceil$ of them. For the coinage pallet, $K = 14$, so the algorithm never generates more than $7$ splits.

[Empirical tests](https://gist.github.com/TorstenStueber/18e1933f2d3669a638c68e945ead6649) give the following distribution of the number of splits on randomly sampled inputs:

| splits | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| frequency | 27.35% | 36.53% | 26.26% | 8.58% | 1.21% | 0.07% | 0.00% |

## Comparison with the current implementation

We now compare the optimal algorithm with the one currently implemented, which is used only when $A$ is not reachable from $\mathbf{n}$:

> **Rule H.** If some coin's value exceeds $A$, divide the smallest such coin. Otherwise take coins in nonincreasing order of denomination for as long as the running total stays strictly below $A$, and divide the first coin that brings the total to $A$ or beyond.

Here *dividing* a coin means splitting it in a minimal way such that the required value $r$ is reachable from the outputs. This always creates $k - z(r)$ new coins, where $k$ is the denomination of the input coin and $z(r)$ is the number of trailing zeros in the binary expansion of $r$. In either branch exactly one coin is split.

Rule H therefore creates at most $K$ coins, while the optimum creates at least one whenever a split is needed at all; so the excess never exceeds $K - 1$. The following example shows that this bound is attained.

**Example 1.** The user holds one coin of denomination $0$ and one of denomination $K$, and $A = 2^{K-1} + 1$. Then $A$ is not reachable from $\mathbf{n}$, and Rule H divides the coin of denomination $K$ with $r = A$. Since $z(A) = 0$, this creates $K$ new coins.

The optimal algorithm creates a single coin, because $F = \{K\}$:

- $A \bmod 2^j = 1 \leq 1 = \Sigma_{<j}(\mathbf{n})$ for $1 \leq j < K$;
- $A \bmod 2^K = 2^{K-1} + 1 > 1 = \Sigma_{<K}(\mathbf{n})$.

For $K = 14$ the excess is $13$ coins, the largest possible.

**Example 2.** The user holds one coin each of denominations $1$, $K-1$, and $K$, and $A = 2^K + 1$. No coin exceeds $A$, so Rule H takes its second branch: it passes over the coin of denomination $K$ and divides the coin of denomination $K-1$ to supply the remaining value $1$. This creates $K - 1$ new coins.

Again the optimal algorithm creates a single coin, because $F = \{1\}$:

- $A \bmod 2^1 = 1 > 0 = \Sigma_{<1}(\mathbf{n})$;
- $A \bmod 2^j = 1 \leq 2 = \Sigma_{<j}(\mathbf{n})$ for $1 < j < K$;
- $A \bmod 2^K = 1 \leq 2 + 2^{K-1} = \Sigma_{<K}(\mathbf{n})$.

These are worst cases. On randomly sampled inputs, Rule H matches the best single-split plan on about $23\%$ of the payments that require a split; when it does not, it creates about $6.7$ coins too many on average, and up to $13$. Measured against the true optimum it creates about $7$ coins too many per payment on average, and is strictly worse on about $96\%$ of such payments.
