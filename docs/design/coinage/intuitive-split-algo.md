**TL;DR:** the current coin-splitting rule picks the wrong coin and can inflate a wallet by up to 14 coins where one would do. Counting the *forced denominations* of the payment amount gives a simple rule that is provably optimal.

## Introduction

When the user agent executes a Coinage payment, it has to solve two problems:

1. Can I select coins that add up to the exact payment amount?
2. If not, how do I break some coins into smaller ones so that an exact payment becomes possible?

Both problems are genuinely hard.

Suppose your purse holds a 50c coin, five 20c coins, a 5c coin, and four 2c coins, and you need to pay 68c. A natural way to look for a selection is the *greedy* approach: repeatedly take the largest coin that does not exceed what is still owed. Here that gives 50c, then skips the 20c coins, then takes the 5c coin and all four 2c coins, for a total of 63c.

A valid selection does exist (three 20c coins and four 2c coins), so the greedy approach fails here. A more involved dynamic programming approach solves the problem correctly.

The general version is harder still. If the denominations are not fixed (as they are for Euro coins) but arbitrary, the question becomes the [Subset Sum Problem](https://en.wikipedia.org/wiki/Subset_sum_problem), which is NP-complete. A polynomial-time algorithm for it would settle P versus NP, one of the [Millennium Prize Problems](https://en.wikipedia.org/wiki/Millennium_Prize_Problems).

Problem 2 contains problem 1: to judge whether a split is good enough, you first have to be able to check whether the resulting coins can pay the amount exactly.

## Coinage

The situation in Coinage is more favorable, because every coin value is a power of two cents: 1c, 2c, 4c, 8c, and so on up to $163.84. We call the exponent the **denomination** of the coin, so 1c has denomination 0, 2c has denomination 1, 64c has denomination 6, and $163.84 has denomination 14.

That property is what makes the greedy approach work for problem 1.

So let us turn to problem 2. Coinage lets a coin be split into smaller coins. The results must again be valid coin values (powers of two) and must add up exactly to the original value, so that a split neither creates nor destroys value. Since a split consumes one coin and produces several, the number of coins the user holds grows by the number of outputs minus one, and that net growth is the figure we quote throughout.

Suppose you have to pay 21c and hold a single 32c coin. You could split it into three 8c coins and eight 1c coins (3 x 8 + 8 x 1 = 32), then pay with two of the 8c coins and five of the 1c coins. That grows the coin count by 10.


You could instead turn the 32c coin into one 16c, one 8c, one 4c, one 2c, and two 1c coins, and pay with the 16c, 4c, and 1c coins. That grows the count by 5.

Both examples are shown in the following pictures, where the red coin is the coin to be split, the orange coins are the created coins and each level represents one denomination.

![Screenshot 2026-09-03 at 19.14.07|1190x600, 30%](upload://7hMG7V1gxwrFGb7OFdscs926XOb.png)     ![Screenshot 2026-09-03 at 19.13.26|660x632, 29%](upload://zsN06cI8F0zGQTRGyz644pRpaID.png)

That second pattern is typical for optimal splits: the original coin is split into 2 coins of some base denomination and then one coin for each intermediate denomination.

The second split is the better one: we want to keep the number of coins in the system small. The more coins a user holds, the more state the client has to track, and the slower the app becomes.

The goal for the Coinage component of the user agent is therefore to keep the number of coins a payment has to create as small as possible.

## The current approach

Any real strategy has to account for the fact that the user already holds several coins, and decide which of them to break. The current approach always picks at most one coin to split, as follows:

- If some coin's value exceeds the payment amount, split the smallest such coin.
- Otherwise add up coins in order, starting with the largest, until the running total reaches or exceeds the payment amount, and split the last coin added.

Once that coin has been chosen, the current approach splits it optimally: it exchanges the coin for the smallest set of coins from which the remaining payment amount can be selected exactly.

**Example 1.** The user holds a $2.56 coin and a 1c coin, and the payment amount is $1.29. Since $2.56 is the smallest coin exceeding $1.29, the first rule applies and that coin is split. The smallest split that allows $1.29 to be paid produces one coin each of $1.28, 64c, 32c, 16c, 8c, 4c, and 2c, together with two 1c coins; the payment then uses the $1.28 coin and one 1c coin. Nine coins replace one, so the coin count grows by 8.

**Example 2.** The user holds coins of $5.12, $2.56, and 2c, and needs to pay $5.13. Now the second rule applies: the $5.12 coin goes toward the payment, and the $2.56 coin is split so that the remaining 1c can be paid. The split is the same as in Example 1, and the count grows by 8 again.

Neither case is optimal. In the first, it is enough to split the $2.56 coin into two $1.28 coins and pay with one of them plus the 1c coin the user already held.

In the second, it is enough to split the 2c coin into two 1c coins and pay with the $5.12 coin plus one of them.

Both alternatives grow the coin count by one instead of eight.

## The optimal approach

### Forced denominations

Start with a simple observation. If the payment amount is odd, at least one 1c coin has to take part in the payment, because every other coin is even. If the user holds no 1c coin, some larger coin has to be split to produce one.

The same reasoning applies further up. Suppose the amount is even but not divisible by four (2c, 6c, 10c, and so on). Every coin of 4c or more is a multiple of 4, so the 1c and 2c coins used in the payment have to account for the remainder of 2 on their own, and must add up to at least 2c. If the user's 1c and 2c coins do not total that much, a larger coin has to be split to produce a 2c coin.

One step further, let r be the remainder of the payment amount when divided by 8. Every coin of 8c or more is a multiple of 8 and cannot affect that remainder, so the 1c, 2c, and 4c coins used in the payment must add up to at least r. If the user does not hold that much in coins below 8c, a larger coin has to be split.

This generalizes to every denomination j. Writing A for the payment amount, the remainder A mod 2^j has to be covered by the coins of denominations 0 through j-1 alone, so those coins must total at least A mod 2^j. (Note that A mod 2^j is simply the lowest j bits of A in binary.) Whenever they do not, a coin of denomination j or larger has to be split, producing a coin of denomination j-1.

A denomination identified this way, one that definitely requires splitting a coin of that denomination or larger, we call a **forced denomination**.

### Runs

Our goal is to create as few coins as possible.

Suppose denominations 1 through 6 (2c through 64c) all turn out to be forced. That happens, for instance, when the payment amount is $1.27 (seven 1-bits in binary) and the user holds 2c, 4c, 8c, 16c, 32c, and two 64c coins.

The user then has to split a coin of 2c or larger, one of 4c or larger, and so on up to one of 64c or larger. Splitting a separate coin for each of these is not necessary: splitting one of the 64c coins satisfies every requirement at once. The best way to split it is into two 1c coins plus one coin each of 2c, 4c, 8c, 16c, and 32c, which grows the coin count by 6.

It is tempting to turn this into a general rule: always split a coin at the largest forced denomination. That is sufficient, but it is not optimal.

Take a case where denominations 1, 2, 4, and 5 are forced, which happens when the user has to pay 27c while holding a 4c coin and a 32c coin. Splitting at the largest forced denomination means splitting the 32c coin, and the only optimal way to do that is into two 1c coins plus one coin each of 2c, 4c, 8c, and 16c. The payment then uses the 1c, 2c, 8c, and 16c coins, and the coin count grows by 5.

A better plan splits two coins instead: the 32c coin into two 8c coins and one 16c coin, and the 4c coin into two 1c coins and one 2c coin. This grows the count by only 4.

Both approaches are shown in the following pictures:

![Screenshot 2026-09-03 at 19.20.14|622x602, 30%](upload://9T4oF5ewALuyfvq1ZDm2YpaxkGs.png)    ![Screenshot 2026-09-03 at 19.20.39|1010x600, 30%](upload://dwyDCmJ72YWisjlaHgIHm9StxgM.png)

The general rule is to look at the **runs** of forced denominations, meaning the maximal groups of consecutive forced denominations. The example above has two runs: 1 through 2, and 4 through 5.

Each run calls for exactly one split. Let a be the lowest denomination of a given run and b the highest. One can show that the user is guaranteed to hold at least one coin of denomination b. That coin is split into two coins of denomination a-1 together with one coin of each denomination from a to b-1.

## Proof and algorithm

A full mathematical proof that this approach is optimal, along with a precise statement of the algorithm, can be found in [this document](https://github.com/paritytech/host-coinage-design/blob/main/formal-split-algo.md).