package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.CoreCoinage
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import uniffi.truapi.NativeCoinage
import uniffi.truapi.NativeSubscription
import java.math.BigInteger
import java.security.SecureRandom
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The core's coinage layer, which holds the wallet's coins, and the conversion between its cents and the payment
 * asset's planks.
 */
@Singleton
class CoinageCore @Inject constructor(
    private val coreCoinage: CoreCoinage,
) {
    suspend fun <T> use(block: suspend (NativeCoinage) -> T): Result<T> =
        coreCoinage.coinage().flatMap { coinage -> runCancellableCatching { block(coinage) } }

    suspend fun units(): Result<CoinageUnits> = use { CoinageUnits(BigInteger(it.assetUnit())) }

    /** [open] subscribes with the observer it is given, which pushes into the flow until the flow is cancelled. */
    fun <T> observe(open: suspend (NativeCoinage, (T) -> Unit) -> NativeSubscription): Flow<T> = flow {
        val coinage = coreCoinage.coinage().getOrElse { throw it }
        emitAll(
            callbackFlow {
                val subscription = open(coinage) { item -> trySend(item) }
                awaitClose { subscription.cancel() }
            }
        )
    }
}

/** The payment asset's planks in one cent, the unit the core counts value in. */
class CoinageUnits(private val unit: BigInteger) {
    fun balanceOf(cents: ULong): Balance = Balance(BigInteger(cents.toString()) * unit)

    fun balanceOfExponent(exponent: UByte): Balance = balanceOf(1UL shl exponent.toInt())

    /** Whole cents of [balance], rounded down: the core moves no fraction of a cent. */
    fun centsOf(balance: Balance): ULong = (balance.value / unit).toString().toULong()
}

/** Ids of the operations the app starts in the core, in the app's own scope. */
object CoinageIds {
    private val random = SecureRandom()

    fun random(): ByteArray = ByteArray(32).also(random::nextBytes)

    /** A stable id for [label], so that a retry joins the operation instead of starting a second one. */
    fun of(label: String): ByteArray = label.encodeToByteArray().blake2b256()

    /** The operation the app's [group] runs as, which a retry under the same group joins. */
    fun of(group: CoinageOperationGroupId): ByteArray = of(group.value)
}
