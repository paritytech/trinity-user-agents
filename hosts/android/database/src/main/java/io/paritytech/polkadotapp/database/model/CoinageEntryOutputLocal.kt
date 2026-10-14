package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index

/**
 * One asset minted by a durable transaction. Always an asset we own, so [derivationIndex] is never null.
 *
 * The unique index on [onChainKey] is the Fresh-outputs invariant made structural: no address is ever the
 * output of two entries.
 */
@Entity(
    tableName = "coinage_entry_output",
    primaryKeys = ["entryId", "position"],
    indices = [
        Index(value = ["onChainKey"], unique = true),
        Index(value = ["entryId"]),
        Index(value = ["assetKind", "installationId", "derivationIndex"]),
    ],
)
class CoinageEntryOutputLocal(
    val entryId: Long,
    val position: Int,
    val assetKind: CoinageAssetKindLocal,
    val installationId: ByteArray,
    val derivationIndex: Int,
    val onChainKey: ByteArray,
)
