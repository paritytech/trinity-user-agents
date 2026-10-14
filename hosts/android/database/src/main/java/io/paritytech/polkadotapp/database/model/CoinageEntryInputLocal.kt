package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index

// One asset consumed by a durable transaction. A null [derivationIndex] marks a coin whose key a peer sent us:
// it is never a local asset, so it appears only here and never as an output. [installationId] is null exactly then.
//
// [onChainKey] is the asset's on-chain identity — the derived account id for a coin, the ring VRF public key
// for a voucher — which is what every invariant check and every chain read keys on.
@Entity(
    tableName = "coinage_entry_input",
    primaryKeys = ["entryId", "position"],
    indices = [
        Index(value = ["onChainKey"]),
        Index(value = ["entryId"]),
    ],
)
class CoinageEntryInputLocal(
    val entryId: Long,
    val position: Int,
    val assetKind: CoinageAssetKindLocal,
    val installationId: ByteArray?,
    val derivationIndex: Int?,
    val onChainKey: ByteArray,
)
