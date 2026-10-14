package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

@Entity(tableName = "coins", primaryKeys = ["installationId", "derivationIndex"])
class CoinLocal(
    val installationId: ByteArray,
    val derivationIndex: Int,
    val accountId: ByteArray,
    val valueExponent: Int,
    /** The last age the chain was seen to hold, never cleared once known — see Coin.Age. */
    val ageValue: Int?,
    /** Whether the chain holds the coin right now. Kept apart from [ageValue] on purpose. */
    val onChain: Boolean,
    /** Percentage in `0..100`, or null when the coin's origin was never observed. */
    val recyclerFungibility: Int?,
    /**
     * SCALE-encoded `Vec<CoinHopLocal>`. Null reads back as no hops, which is why a coin minted before this
     * column existed needs no backfill.
     */
    val hops: ByteArray?,
    /** How many coins were claimed alongside this one, kept until [hops] can be built from the coin's age. */
    val incomingBundleSize: Int?,
)
