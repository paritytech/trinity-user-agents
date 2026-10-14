package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash

private const val SHORT_HASH_LENGTH = 10

/**
 * The identity every entry-scoped line carries, so one entry's whole journey greps out of a log export that
 * interleaves every entry the device is tracking.
 */
internal fun coinageLogId(
    id: CoinageTransactionId,
    txHash: TransactionHash? = null,
    groupId: CoinageOperationGroupId? = null,
): String = buildString {
    append("entry=").append(id.value)
    txHash?.let { append(" tx=").append(it.shortHash()) }
    groupId?.let { append(" group=").append(it.value) }
}

internal fun LedgerEntry.logId(): String = coinageLogId(id, txHash, groupId)

/** Enough of a hash to tell it apart from the others in the log, without filling the line with it. */
internal fun String.shortHash(): String = take(SHORT_HASH_LENGTH)

internal fun AssetPublicKey.shortKey(): String = toString().shortHash()

internal fun LedgerAsset.describe(): String = "${asset ?: kind}@${publicKey.shortKey()}"
