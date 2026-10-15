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
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
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
import kotlinx.coroutines.flow.collectLatest
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
 *
 * The core never says when a product stops following the balance, so the
 * balance is pushed while the app is in the foreground, and pushed again on
 * returning to it.
 */
@Singleton
class TrUAPIPaymentPlatforms @Inject constructor(
    private val topUpService: TopUpService,
    private val requestPaymentUseCase: RequestPaymentUseCase,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    private val appLifecycleObserver: AppLifecycleObserver,
    dispatchers: CoroutineDispatchers,
) : TopUpHostBridge, PaymentHostBridge, BalanceHostBridge {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    @Volatile
    private var sink: TrUAPIPaymentSink? = null

    private val topUps = StatusForwarder<TopUpStatus>(
        scope = scope,
        isTerminal = { it.isTerminal },
        isNotFound = { it is TopUpError.NotFound },
        unknownYet = TopUpStatus.Detecting,
        notify = { productId, id, status -> sink?.topUpStatus(productId, id, status.toNative()) },
    )

    private val payments = StatusForwarder<PaymentStatus>(
        scope = scope,
        isTerminal = { it !is PaymentStatus.Processing },
        isNotFound = { it is PaymentRequestError.NotFound },
        unknownYet = PaymentStatus.Processing,
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
        val product = ProductId.fromStoredValue(productId)
        val id = startTopUp(product, request).getOrElse { throw it }

        topUps.forward(productId, request.id) { topUpService.status(product, id) }
    }

    private suspend fun startTopUp(product: ProductId, request: HostPaymentTopUpRequest): Result<PaymentTopUpId> {
        if (request.into != null) return Result.failure(HostPaymentTopUpException.Unknown(NO_PURSES))

        val id = PaymentTopUpId.fromBytes(request.id.toDataByteArray())
            .getOrElse { return Result.failure(HostPaymentTopUpException.Unknown(it.message ?: "invalid top-up id")) }
        val source = request.source.toDomain()
            .getOrElse { return Result.failure(HostPaymentTopUpException.InvalidSource()) }

        return topUpService.start(product, id, request.amount.toBalance(), source)
            .fold(onSuccess = { Result.success(id) }, onFailure = { Result.failure(it.asTopUpException()) })
    }

    override fun topUpStatus(productId: String, id: ByteArray): HostPaymentTopUpStatusSubscribeItem? {
        val topUpId = PaymentTopUpId.fromBytes(id.toDataByteArray()).getOrElse { return null }

        return topUps.current(productId, id) {
            topUpService.status(ProductId.fromStoredValue(productId), topUpId)
        }?.toNative()
    }

    override suspend fun requestPayment(productId: String, request: HostPaymentRequest) {
        val product = ProductId.fromStoredValue(productId)
        val id = startPayment(product, request).getOrElse { throw it }

        payments.forward(productId, request.id) { requestPaymentUseCase.subscribeStatus(product, id) }
    }

    private suspend fun startPayment(product: ProductId, request: HostPaymentRequest): Result<ProductPaymentRequestId> {
        if (request.from != null) return Result.failure(HostPaymentException.Unknown(NO_PURSES))

        val id = ProductPaymentRequestId.fromBytes(request.id.toDataByteArray())
            .getOrElse { return Result.failure(HostPaymentException.Unknown(it.message ?: "invalid payment id")) }

        return requestPaymentUseCase.requestPayment(
            productId = product,
            id = id,
            amount = request.amount.toBalance(),
            destination = request.destination.intoAccountId(),
            shortfallDisclosure = ShortfallDisclosure.CALLER,
        ).fold(onSuccess = { Result.success(id) }, onFailure = { Result.failure(it.asPaymentException()) })
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
            appLifecycleObserver.subscribeIsForeground().collectLatest { foreground ->
                if (!foreground) return@collectLatest
                totalBalanceUseCase.subscribeSpendableByProducts()
                    .distinctUntilChanged()
                    .collect { sink?.balance(it) }
            }
        }
    }

    private companion object {
        const val NO_PURSES = "the host has only the main purse"
    }
}

/**
 * One run per (product, id) forwarding an engine's statuses until terminal, and
 * answering the core's synchronous ask from that run. A final status is kept,
 * so asking again about a settled operation answers at once; a run that ends
 * otherwise is forgotten, so the next ask reads the engine again.
 *
 * The ask runs on the core's thread, so it waits on the engine only briefly.
 * An operation the engine has not answered for by then reads as [unknownYet],
 * and its real status is pushed once the engine has it.
 */
internal class StatusForwarder<S : Any>(
    private val scope: CoroutineScope,
    private val isTerminal: (S) -> Boolean,
    private val isNotFound: (Throwable) -> Boolean,
    private val unknownYet: S,
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

    private val settled = object : LinkedHashMap<Key, S>() {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Key, S>) = size > SETTLED_KEPT
    }

    fun forward(productId: String, id: ByteArray, statuses: () -> Flow<S>) {
        follow(productId, id, statuses)
    }

    /** `null` when the engine knows no such operation. */
    fun current(productId: String, id: ByteArray, statuses: () -> Flow<S>): S? {
        synchronized(settled) { settled[keyOf(productId, id)] }?.let { return it }

        val run = follow(productId, id, statuses)
        val latest = run.value.takeIf { it !is Latest.Pending } ?: runBlocking {
            withTimeoutOrNull(FIRST_STATUS_WAIT_MS) { run.first { it !is Latest.Pending } }
        } ?: Latest.Pending

        @Suppress("UNCHECKED_CAST")
        return when (latest) {
            is Latest.Known<*> -> latest.status as S
            Latest.NotFound -> null
            is Latest.Failed -> throw latest.error
            Latest.Pending -> unknownYet
        }
    }

    private fun keyOf(productId: String, id: ByteArray) = Key(productId, id.copyOf().toDataByteArray())

    private fun follow(productId: String, id: ByteArray, statuses: () -> Flow<S>): MutableStateFlow<Latest<S>> {
        val key = keyOf(productId, id)
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
                        if (isTerminal(status)) synchronized(settled) { settled[key] = status }
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

        const val FIRST_STATUS_WAIT_MS = 1_000L

        const val SETTLED_KEPT = 256
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
    is PaymentStatus.PartiallyClaimed -> HostPaymentStatusSubscribeItem.ClaimedPartially(actualClaimed = claimed.value.toString())
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
