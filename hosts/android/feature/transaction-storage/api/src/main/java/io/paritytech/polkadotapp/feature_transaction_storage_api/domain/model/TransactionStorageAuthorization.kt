package io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.atLeastZero
import kotlinx.serialization.Serializable
import java.math.BigInteger

@Serializable
data class TransactionStorageAuthorization(
    val extent: TransactionStorageExtent,
    val expiration: BlockNumber,
)

@Serializable
data class TransactionStorageExtent(
    val transactions: BigIntegerSerializable,
    val transactionsAllowance: BigIntegerSerializable,
    val bytes: BigIntegerSerializable,
    val extra: TransactionStorageExtentExtra,
    val bytesAllowance: BigIntegerSerializable
)

// Mirrors Bulletin `AuthorizationExtent<Extra>` with `Extra = PermanentExtent { bytes_permanent }`
// (bulletin-paseo runtime >= 1_000_026).
@Serializable
data class TransactionStorageExtentExtra(
    val bytesPermanent: BigIntegerSerializable
)

val TransactionStorageExtent.remainingTransactions: BigInteger
    get() = (transactionsAllowance - transactions).atLeastZero()

val TransactionStorageExtent.remainingBytes: BigInteger
    get() = (bytesAllowance - bytes).atLeastZero()

fun TransactionStorageAuthorization.hasExpiredAt(blockNumber: BlockNumber): Boolean {
    return blockNumber > expiration
}

fun TransactionStorageAuthorization.hasCapacityFor(size: InformationSize): Boolean {
    return extent.remainingTransactions > BigInteger.ZERO && extent.remainingBytes.toLong() >= size.inWholeBytes
}

fun TransactionStorageAuthorization.storedTransactionAfter(previousTransactionsCount: BigInteger): Boolean {
    return extent.transactionsAllowance < previousTransactionsCount
}

fun TransactionStorageAuthorization.increasedAllocationAfter(previousTransactionsCount: BigInteger): Boolean {
    return extent.transactionsAllowance > previousTransactionsCount
}
