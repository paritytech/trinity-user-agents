package io.paritytech.polkadotapp.database.model

/**
 * Which kind of coinage asset a ledger row names.
 *
 * Standalone rather than nested in the transaction row: the transaction itself is the durability engine's,
 * shared by every domain, while the kind is coinage's alone.
 */
enum class CoinageAssetKindLocal {
    COIN,
    VOUCHER,
}

class BlockRefLocal(
    val blockNumber: Long,
    val blockHash: String,
)
