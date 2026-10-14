package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

// A coinage derivation subtree this account is known to own: the current installation's, which is the only one
// new keys are allocated in, or a previous installation's, which is only ever scanned for balance.
//
// The scan progress belongs to previous installations; the current one never needs scanning since this device
// holds every key it allocated there.
@Entity(tableName = "coinage_installations")
class CoinageInstallationLocal(
    @PrimaryKey val installationId: ByteArray,
    val isCurrent: Boolean,
    val coinScanNextIndex: Int,
    val voucherScanNextIndex: Int,
    val initialScanCompleted: Boolean,
)
