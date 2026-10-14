package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

data class Coin(
    val derivationIndex: CoinageKeyIndex,
    val valueExponent: ValueExponent,
    val age: Age,
    val isOnChain: Boolean,
    val accountId: AccountId,
    val provenance: CoinProvenance
) {
    /**
     * The last age the chain was seen to hold for this coin, and never cleared once known.
     *
     * Presence is [isOnChain] and lives apart from this on purpose. Reading the two off one value cannot
     * tell a coin that is not there from one nothing has looked at yet, and those call for opposite
     * conclusions: the first says a peer took it, the second says it is too early to say anything.
     */
    sealed interface Age {
        data object Unknown : Age
        data class Known(val value: Int) : Age
    }
}

/**
 * Whether the chain has ever been seen to hold this coin.
 *
 * False means nothing has observed it yet, so its absence is ignorance rather than evidence — no conclusion
 * that rests on the coin being gone may be drawn.
 */
val Coin.hasEverBeenOnChain: Boolean get() = age is Coin.Age.Known

fun Coin.knownAgeOrThrow() = age as Coin.Age.Known

fun Coin.tokenAmount() = valueExponent.tokenAmount()

val Coin.hops: List<Hop> get() = provenance.hops

val Coin.recyclerFungibility: RecyclerFungibility? get() = provenance.recyclerFungibility

fun Coin.isAgeValidToSpend(recyclableAge: Int) = when (age) {
    is Coin.Age.Known -> age.value < recyclableAge
    Coin.Age.Unknown -> false
}

fun Coin.ageOrDefault() = (this.age as? Coin.Age.Known)?.value ?: -1

fun Coin.ageOrNull(): Int? = (age as? Coin.Age.Known)?.value

fun List<Coin>.filterSpendable(recyclableAge: Int): List<Coin> {
    return filter { it.isAgeValidToSpend(recyclableAge) }
}
