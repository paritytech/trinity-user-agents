package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.BalanceHostBridge
import io.parity.truapi.PaymentHostBridge
import io.parity.truapi.TopUpHostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestError
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ShortfallDisclosure
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.subscribeSpendableByProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpError
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.isTerminal
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.transformWhile
import timber.log.Timber
import uniffi.truapi.HostPaymentException
import uniffi.truapi.HostPaymentRequest
import uniffi.truapi.HostPaymentStatusSubscribeException
import uniffi.truapi.HostPaymentStatusSubscribeItem
import uniffi.truapi.HostPaymentTopUpException
import uniffi.truapi.HostPaymentTopUpRequest
import uniffi.truapi.HostPaymentTopUpStatusSubscribeException
import uniffi.truapi.HostPaymentTopUpStatusSubscribeItem
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.PaymentTopUpSource as NativeTopUpSource

internal interface TrUAPIPaymentSink {
    fun topUpStatus(productId: ProductId, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem)

    fun paymentStatus(productId: ProductId, id: ByteArray, status: HostPaymentStatusSubscribeItem)

    fun balance(available: Balance)
}

/**
 * Serves the core's top-up, payment and balance platforms from the same top-up,
 * payment and balance engines the app's other product calls use. The core owns
 * BalanceAccess, so nothing here prompts for it.
 *
 * The core decides which operations and when the balance to follow, and keeps
 * the latest status of each. A follow call forwards the engine's statuses until
 * a terminal one or the end of its stream; the core cancels it when done.
 */
@Singleton
class TrUAPIPaymentPlatforms @Inject constructor(
    private val topUpService: TopUpService,
    private val requestPaymentUseCase: RequestPaymentUseCase,
    private val totalBalanceUseCase: TotalBalanceUseCase,
) : TopUpHostBridge, PaymentHostBridge, BalanceHostBridge {
    @Volatile
    private var sink: TrUAPIPaymentSink? = null

    fun install(runtime: TrUAPIHostRuntime) {
        attach(
            object : TrUAPIPaymentSink {
                override fun topUpStatus(productId: ProductId, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem) =
                    runtime.notifyTopUpStatus(productId, id, status)

                override fun paymentStatus(productId: ProductId, id: ByteArray, status: HostPaymentStatusSubscribeItem) =
                    runtime.notifyPaymentStatus(productId, id, status)

                override fun balance(available: Balance) =
                    runtime.notifyBalance(available.value.toString())
            }
        )
        runtime.setTopUp(this)
        runtime.setPayments(this)
        runtime.setBalance(this)
    }

    internal fun attach(sink: TrUAPIPaymentSink) {
        this.sink = sink
    }

    override suspend fun topUp(productId: ProductId, request: HostPaymentTopUpRequest) {
        startTopUp(productId, request).getOrElse { throw it }
    }

    private suspend fun startTopUp(product: ProductId, request: HostPaymentTopUpRequest): Result<PaymentTopUpId> {
        if (request.into != null) return Result.failure(HostPaymentTopUpException.Unknown(NO_PURSES))

        val id = PaymentTopUpId.fromBytes(request.id.toDataByteArray())
            .getOrElse { return Result.failure(HostPaymentTopUpException.Unknown(INVALID_ID)) }
        val amount = request.amount.toPlanksOrNull() ?: return Result.failure(HostPaymentTopUpException.InvalidAmount())
        val source = request.source.toDomain()
            .getOrElse { return Result.failure(HostPaymentTopUpException.InvalidSource()) }

        return topUpService.start(product, id, amount, source)
            .fold(onSuccess = { Result.success(id) }, onFailure = { Result.failure(it.asTopUpException()) })
    }

    override suspend fun followTopUp(productId: ProductId, id: ByteArray) {
        val topUpId = PaymentTopUpId.fromBytes(id.toDataByteArray())
            .getOrElse { throw HostPaymentTopUpStatusSubscribeException.TopUpNotFound() }

        forward(
            statuses = topUpService.status(productId, topUpId),
            isTerminal = { it.isTerminal },
            push = { sink?.topUpStatus(productId, id, it.toNative()) },
            failure = { error ->
                if (error is TopUpError.NotFound) {
                    HostPaymentTopUpStatusSubscribeException.TopUpNotFound()
                } else {
                    Timber.tag(PAYMENTS_LOG_TAG).w(error, "Top-up status stream failed for $productId")
                    HostPaymentTopUpStatusSubscribeException.Unknown(STATUS_UNAVAILABLE)
                }
            },
        )
    }

    override suspend fun requestPayment(productId: ProductId, request: HostPaymentRequest) {
        startPayment(productId, request).getOrElse { throw it }
    }

    private suspend fun startPayment(product: ProductId, request: HostPaymentRequest): Result<ProductPaymentRequestId> {
        if (request.from != null) return Result.failure(HostPaymentException.Unknown(NO_PURSES))

        val id = ProductPaymentRequestId.fromBytes(request.id.toDataByteArray())
            .getOrElse { return Result.failure(HostPaymentException.Unknown(INVALID_ID)) }
        val amount = request.amount.toPlanksOrNull() ?: return Result.failure(HostPaymentException.InvalidAmount())

        return requestPaymentUseCase.requestPayment(
            productId = product,
            id = id,
            amount = amount,
            destination = request.destination.intoAccountId(),
            shortfallDisclosure = ShortfallDisclosure.MASKED_BY_CORE,
        ).fold(onSuccess = { Result.success(id) }, onFailure = { Result.failure(it.asPaymentException()) })
    }

    override suspend fun followPayment(productId: ProductId, id: ByteArray) {
        val paymentId = ProductPaymentRequestId.fromBytes(id.toDataByteArray())
            .getOrElse { throw HostPaymentStatusSubscribeException.PaymentNotFound() }

        forward(
            statuses = requestPaymentUseCase.subscribeStatus(productId, paymentId),
            isTerminal = { it !is PaymentStatus.Processing },
            push = { status ->
                if (status is PaymentStatus.Failed) Timber.tag(PAYMENTS_LOG_TAG).w("Payment failed: %s", status.reason)
                sink?.paymentStatus(productId, id, status.toNative())
            },
            failure = { error ->
                if (error is PaymentRequestError.NotFound) {
                    HostPaymentStatusSubscribeException.PaymentNotFound()
                } else {
                    Timber.tag(PAYMENTS_LOG_TAG).w(error, "Payment status stream failed for $productId")
                    HostPaymentStatusSubscribeException.Unknown(STATUS_UNAVAILABLE)
                }
            },
        )
    }

    override suspend fun followBalance() {
        forward(
            statuses = totalBalanceUseCase.subscribeSpendableByProducts().distinctUntilChanged(),
            isTerminal = { false },
            push = { sink?.balance(it) },
            failure = { error ->
                Timber.tag(PAYMENTS_LOG_TAG).w(error, "Balance stream failed")
                HostRejection.Rejected(BALANCE_UNAVAILABLE)
            },
        )
    }

    /** Push each of [statuses] until a terminal one or the stream's end, throwing [failure] of an engine error. */
    private suspend fun <S> forward(
        statuses: Flow<S>,
        isTerminal: (S) -> Boolean,
        push: (S) -> Unit,
        failure: (Throwable) -> Throwable,
    ) {
        try {
            statuses
                .transformWhile { status ->
                    emit(status)
                    !isTerminal(status)
                }
                .collect { push(it) }
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (error: Throwable) {
            throw failure(error)
        }
    }

    private companion object {
        const val NO_PURSES = "the host has only the main purse"

        const val INVALID_ID = "the id is not 32 bytes"

        const val BALANCE_UNAVAILABLE = "balance is unavailable"
    }
}

/** The amount as planks, or `null` when it is not a positive whole number. */
private fun String.toPlanksOrNull(): Balance? = toBigIntegerOrNull()?.takeIf { it.signum() > 0 }?.intoBalance()

private const val PAYMENTS_LOG_TAG = "truapi.payments"

// Products see these fixed reasons. The engine's own text, which can name internals or a short balance the
// core would otherwise mask, only reaches the log.
private const val TOP_UP_FAILED = "top-up could not be registered"
private const val PAYMENT_FAILED = "payment could not be started"
private const val PAYMENT_DID_NOT_COMPLETE = "payment did not complete"
private const val STATUS_UNAVAILABLE = "status is unavailable"

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
    is PaymentStatus.Failed -> HostPaymentStatusSubscribeItem.Failed(reason = PAYMENT_DID_NOT_COMPLETE)
}

private fun Throwable.asTopUpException(): HostPaymentTopUpException = when (this) {
    is TopUpError.InvalidSource -> HostPaymentTopUpException.InvalidSource()
    is TopUpError.AlreadyExists -> HostPaymentTopUpException.AlreadyExists()
    is TopUpError.SourceBusy -> HostPaymentTopUpException.SourceBusy()
    else -> {
        Timber.tag(PAYMENTS_LOG_TAG).w(this, "Top-up could not be registered")
        HostPaymentTopUpException.Unknown(TOP_UP_FAILED)
    }
}

private fun Throwable.asPaymentException(): HostPaymentException = when (this) {
    is PaymentRequestError.AlreadyExists -> HostPaymentException.AlreadyExists()
    is PaymentRequestError.Rejected -> HostPaymentException.Rejected()
    is PaymentRequestError.InsufficientBalance -> HostPaymentException.InsufficientBalance()
    else -> {
        Timber.tag(PAYMENTS_LOG_TAG).w(this, "Payment could not be started")
        HostPaymentException.Unknown(PAYMENT_FAILED)
    }
}
