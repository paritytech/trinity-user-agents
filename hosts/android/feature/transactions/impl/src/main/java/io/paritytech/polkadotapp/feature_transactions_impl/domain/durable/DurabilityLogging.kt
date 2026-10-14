package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DURABILITY_LOG_TAG
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import timber.log.Timber

private const val SHORT_HASH_LENGTH = 10

fun durabilityLogD(message: String) {
    Timber.tag(DURABILITY_LOG_TAG).d(message)
}

fun durabilityLogI(message: String) {
    Timber.tag(DURABILITY_LOG_TAG).i(message)
}

fun durabilityLogW(message: String, throwable: Throwable? = null) {
    Timber.tag(DURABILITY_LOG_TAG).w(throwable, message)
}

fun durabilityLogE(message: String, throwable: Throwable? = null) {
    Timber.tag(DURABILITY_LOG_TAG).e(throwable, message)
}

/**
 * The identity every transaction-scoped line carries, so one transaction's whole journey greps out of an
 * export that interleaves every transaction the device is tracking.
 */
internal fun durabilityLogId(
    domainId: TxDomainId,
    id: DurableTxId,
    txHash: TransactionHash? = null,
    groupId: OperationGroupId? = null,
): String = buildString {
    append(domainId.value).append("/entry=").append(id.value)
    txHash?.let { append(" tx=").append(it.shortHash()) }
    groupId?.let { append(" group=").append(it.value) }
}

internal fun DurableTxEntry.logId(): String = durabilityLogId(domainId, id, txHash, groupId)

/** Enough of a hash to tell it apart from the others in the log, without filling the line with it. */
internal fun String.shortHash(): String = take(SHORT_HASH_LENGTH)
