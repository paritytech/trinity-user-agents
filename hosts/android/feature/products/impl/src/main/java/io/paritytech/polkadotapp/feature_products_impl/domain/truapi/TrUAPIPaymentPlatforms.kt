package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.BalanceHostBridge
import io.parity.truapi.PaymentHostBridge
import io.parity.truapi.TopUpHostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestError
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ShortfallDisclosure
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.spendableByProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.subscribeSpendableByProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpError
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.isTerminal
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.HostPaymentBalanceSubscribeException
import uniffi.truapi.HostPaymentException
import uniffi.truapi.HostPaymentRequest
import uniffi.truapi.HostPaymentStatusSubscribeItem
import uniffi.truapi.HostPaymentTopUpException
import uniffi.truapi.HostPaymentTopUpRequest
import uniffi.truapi.HostPaymentTopUpStatusSubscribeItem
import java.math.BigInteger
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.PaymentTopUpSource as NativeTopUpSource

internal interface TrUAPIPaymentSink {
    fun topUpStatus(productId: String, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem)

    fun paymentStatus(productId: String, id: ByteArray, status: HostPaymentStatusSubscribeItem)

    fun balance(available: Balance)
}

/**
 * Serves the core's top-up, payment and balance platforms from the engines the
 * legacy host calls use. The core owns BalanceAccess, so nothing here prompts
 * for it.
 *
 * The core asks for an operation's current status once and expects every later
 * one pushed, so a start, or the first ask after a restart, forwards that
 * operation's engine status until it is terminal.
 */
@Singleton
class TrUAPIPaymentPlatforms @Inject constructor(
    private val topUpService: TopUpService,
    private val requestPaymentUseCase: RequestPaymentUseCase,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    dispatchers: CoroutineDispatchers,
) : TopUpHostBridge, PaymentHostBridge, BalanceHostBridge {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    @Volatile
    private var sink: TrUAPIPaymentSink? = null

    private val topUps = StatusForwarder<TopUpStatus>(
        scope = scope,
        isTerminal = { it.isTerminal },
        isNotFound = { it is TopUpError.NotFound },
        notify = { productId, id, status -> sink?.topUpStatus(productId, id, status.toNative()) },
    )

    private val payments = StatusForwarder<PaymentStatus>(
        scope = scope,
        isTerminal = { it !is PaymentStatus.Processing },
        isNotFound = { it is PaymentRequestError.NotFound },
        notify = { productId, id, status -> sink?.paymentStatus(productId, id, status.toNative()) },
    )

    private val balanceForwarding = AtomicBoolean(false)

    fun install(runtime: TrUAPIHostRuntime) {
        attach(
            object : TrUAPIPaymentSink {
                override fun topUpStatus(productId: String, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem) =
                    runtime.notifyTopUpStatus(productId, id, status)

                override fun paymentStatus(productId: String, id: ByteArray, status: HostPaymentStatusSubscribeItem) =
                    runtime.notifyPaymentStatus(productId, id, status)

                override fun balance(available: Balance) =
                    runtime.notifyBalance(purse = null, available = available.value.toString())
            }
        )
        runtime.setTopUp(this)
        runtime.setPayments(this)
        runtime.setBalance(this)
    }

    internal fun attach(sink: TrUAPIPaymentSink) {
        this.sink = sink
    }

    override suspend fun topUp(productId: String, request: HostPaymentTopUpRequest) {
        if (request.into != null) throw HostPaymentTopUpException.Unknown(NO_PURSES)

        val product = ProductId.fromStoredValue(productId)
        val id = PaymentTopUpId.fromBytes(request.id.toDataByteArray())
            .getOrElse { throw HostPaymentTopUpException.Unknown(it.message ?: "invalid top-up id") }
        val source = request.source.toDomain()
            .getOrElse { throw HostPaymentTopUpException.InvalidSource() }

        topUpService.start(product, id, request.amount.toBalance(), source)
            .getOrElse { throw it.asTopUpException() }

        topUps.forward(productId, request.id) { topUpService.status(product, id) }
    }

    override fun topUpStatus(productId: String, id: ByteArray): HostPaymentTopUpStatusSubscribeItem? {
        val topUpId = PaymentTopUpId.fromBytes(id.toDataByteArray()).getOrElse { return null }

        return topUps.current(productId, id) {
            topUpService.status(ProductId.fromStoredValue(productId), topUpId)
        }?.toNative()
    }

    override suspend fun requestPayment(productId: String, request: HostPaymentRequest) {
        if (request.from != null) throw HostPaymentException.Unknown(NO_PURSES)

        val product = ProductId.fromStoredValue(productId)
        val id = ProductPaymentRequestId.fromBytes(request.id.toDataByteArray())
            .getOrElse { throw HostPaymentException.Unknown(it.message ?: "invalid payment id") }

        requestPaymentUseCase.requestPayment(
            productId = product,
            id = id,
            amount = request.amount.toBalance(),
            destination = request.destination.intoAccountId(),
            shortfallDisclosure = ShortfallDisclosure.CALLER,
        ).getOrElse { throw it.asPaymentException() }

        payments.forward(productId, request.id) { requestPaymentUseCase.subscribeStatus(product, id) }
    }

    override fun paymentStatus(productId: String, id: ByteArray): HostPaymentStatusSubscribeItem? {
        val paymentId = ProductPaymentRequestId.fromBytes(id.toDataByteArray()).getOrElse { return null }

        return payments.current(productId, id) {
            requestPaymentUseCase.subscribeStatus(ProductId.fromStoredValue(productId), paymentId)
        }?.toNative()
    }

    override suspend fun balance(productId: String, purse: UInt?): String {
        if (purse != null) throw HostPaymentBalanceSubscribeException.Unknown(NO_PURSES)

        // The core registers its follower before asking, so changes from here on reach it.
        forwardBalance()

        return totalBalanceUseCase.getBalance()
            .map { it.spendableByProducts().value.toString() }
            .getOrElse { throw HostPaymentBalanceSubscribeException.Unknown(it.message ?: "balance unavailable") }
    }

    private fun forwardBalance() {
        if (!balanceForwarding.compareAndSet(false, true)) return

        scope.launch {
            totalBalanceUseCase.subscribeSpendableByProducts()
                .distinctUntilChanged()
                .collect { sink?.balance(it) }
        }
    }

    private companion object {
        const val NO_PURSES = "the host has only the main purse"
    }
}

/**
 * One run per (product, id) forwarding an engine's statuses until terminal, and
 * answering the core's synchronous ask from that run. A run that ends is
 * forgotten, so the next ask reads the engine again.
 */
internal class StatusForwarder<S : Any>(
    private val scope: CoroutineScope,
    private val isTerminal: (S) -> Boolean,
    private val isNotFound: (Throwable) -> Boolean,
    private val notify: (productId: String, id: ByteArray, status: S) -> Unit,
) {
    private sealed interface Latest<out T> {
        data object Pending : Latest<Nothing>

        data class Known<T>(val status: T) : Latest<T>

        data object NotFound : Latest<Nothing>

        data class Failed(val error: Throwable) : Latest<Nothing>
    }

    private data class Key(val productId: String, val id: DataByteArray)

    private val runs = ConcurrentHashMap<Key, MutableStateFlow<Latest<S>>>()

    fun forward(productId: String, id: ByteArray, statuses: () -> Flow<S>) {
        follow(productId, id, statuses)
    }

    /** `null` when the engine knows no such operation. Blocks the core's thread until the engine answers. */
    fun current(productId: String, id: ByteArray, statuses: () -> Flow<S>): S? {
        val latest = runBlocking {
            withTimeoutOrNull(CURRENT_STATUS_TIMEOUT_MS) {
                follow(productId, id, statuses).first { it !is Latest.Pending }
            }
        } ?: throw IllegalStateException("status unavailable after ${CURRENT_STATUS_TIMEOUT_MS}ms")

        @Suppress("UNCHECKED_CAST")
        return when (latest) {
            is Latest.Known<*> -> latest.status as S
            Latest.NotFound -> null
            is Latest.Failed -> throw latest.error
            Latest.Pending -> error("unreachable")
        }
    }

    private fun follow(productId: String, id: ByteArray, statuses: () -> Flow<S>): MutableStateFlow<Latest<S>> {
        val key = Key(productId, id.copyOf().toDataByteArray())
        val fresh = MutableStateFlow<Latest<S>>(Latest.Pending)
        runs.putIfAbsent(key, fresh)?.let { return it }

        scope.launch {
            try {
                statuses()
                    .transformWhile { status ->
                        emit(status)
                        !isTerminal(status)
                    }
                    .collect { status ->
                        fresh.value = Latest.Known(status)
                        notify(productId, id, status)
                    }
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (error: Throwable) {
                val notFound = isNotFound(error)
                if (!notFound) Timber.tag(LOG_TAG).w(error, "Status forwarding failed for $productId")
                fresh.value = if (notFound) Latest.NotFound else Latest.Failed(error)
            } finally {
                runs.remove(key, fresh)
            }
        }

        return fresh
    }

    private companion object {
        const val LOG_TAG = "truapi.payments"

        const val CURRENT_STATUS_TIMEOUT_MS = 10_000L
    }
}

private fun String.toBalance(): Balance = BigInteger(this).intoBalance()

private fun NativeTopUpSource.toDomain(): Result<PaymentTopUpSource> = when (this) {
    is NativeTopUpSource.ProductAccount -> derivationIndex.toDomain().map { PaymentTopUpSource.ProductAccount(it) }
    is NativeTopUpSource.PrivateKey -> Result.success(PaymentTopUpSource.PrivateKey(sr25519SecretKey.toDataByteArray()))
    is NativeTopUpSource.Coins -> Result.success(PaymentTopUpSource.Coins(sr25519SecretKeys.map { it.toDataByteArray() }))
}

private fun TopUpStatus.toNative(): HostPaymentTopUpStatusSubscribeItem = when (this) {
    TopUpStatus.Detecting -> HostPaymentTopUpStatusSubscribeItem.Detecting
    TopUpStatus.Claiming -> HostPaymentTopUpStatusSubscribeItem.Claiming
    is TopUpStatus.Claimed -> HostPaymentTopUpStatusSubscribeItem.Claimed(finalized = finalized)
    is TopUpStatus.ClaimedPartially ->
        HostPaymentTopUpStatusSubscribeItem.ClaimedPartially(actualClaimed = actualClaimed.value.toString())
    TopUpStatus.NotClaimed -> HostPaymentTopUpStatusSubscribeItem.NotClaimed
}

private fun PaymentStatus.toNative(): HostPaymentStatusSubscribeItem = when (this) {
    PaymentStatus.Processing -> HostPaymentStatusSubscribeItem.Processing
    PaymentStatus.Completed -> HostPaymentStatusSubscribeItem.Completed
    is PaymentStatus.PartiallyClaimed -> HostPaymentStatusSubscribeItem.PartiallyClaimed(actualClaimed = claimed.value.toString())
    is PaymentStatus.Failed -> HostPaymentStatusSubscribeItem.Failed(reason = reason)
}

private fun Throwable.asTopUpException(): HostPaymentTopUpException = when (this) {
    is TopUpError.InvalidSource -> HostPaymentTopUpException.InvalidSource()
    is TopUpError.AlreadyExists -> HostPaymentTopUpException.AlreadyExists()
    is TopUpError.SourceBusy -> HostPaymentTopUpException.SourceBusy()
    else -> HostPaymentTopUpException.Unknown(message ?: toString())
}

private fun Throwable.asPaymentException(): HostPaymentException = when (this) {
    is PaymentRequestError.AlreadyExists -> HostPaymentException.AlreadyExists()
    is PaymentRequestError.Rejected -> HostPaymentException.Rejected()
    is PaymentRequestError.InsufficientBalance -> HostPaymentException.InsufficientBalance()
    else -> HostPaymentException.Unknown(message ?: toString())
}
