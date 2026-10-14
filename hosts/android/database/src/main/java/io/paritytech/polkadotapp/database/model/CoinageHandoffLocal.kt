package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

/**
 * A coin whose private key is leaving, or has left, the device over an off-chain channel.
 *
 * Written before the bytes reach the transport, because a key that reaches a peer without a mark can be
 * selected again and double-spent. [committed] is what separates the two moments: until the carrier of the
 * keys is durable the mark is provisional, and a relaunch clears it so the coins come back rather than being
 * frozen by a payment that never happened. Once committed it is never retracted — a peer holding the key can
 * transfer out at any moment.
 */
@Entity(
    tableName = "coinage_handoff",
    indices = [Index(value = ["assetKind", "installationId", "derivationIndex"])],
)
class CoinageHandoffLocal(
    @PrimaryKey val onChainKey: ByteArray,
    val assetKind: CoinageAssetKindLocal,
    val installationId: ByteArray,
    val derivationIndex: Int,
    val committed: Boolean,
)
