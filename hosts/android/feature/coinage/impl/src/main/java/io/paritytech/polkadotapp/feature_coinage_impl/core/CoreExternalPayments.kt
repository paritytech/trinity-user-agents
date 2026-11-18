package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import uniffi.truapi.CoinageException.UnknownOperation
import uniffi.truapi.NativeStatusObserver
import uniffi.truapi.Retry
import uniffi.truapi.Started
import uniffi.truapi.Status
import uniffi.truapi.StatusItem
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes
import kotlin.time.ExperimentalTime

class CoreExternalPaymentPlanner @Inject constructor(
    private val totalBalanceUseCase: TotalBalanceUseCase,
) : ExternalPaymentPlanner {
    override suspend fun plan(amount: Balance): Result<ExternalPaymentPlan> = totalBalanceUseCase.getBalance().map { balance ->
        when {
            amount <= balance.availablePrivate -> ExternalPaymentPlan.Private
            amount <= balance.available -> ExternalPaymentPlan.SpendsGainingPrivacy
            else -> ExternalPaymentPlan.NotEnoughAmount(balance.available)
        }
    }
}

@OptIn(ExperimentalTime::class)
class CoreExternalPaymentService @Inject constructor(
    private val core: CoinageCore,
    private val timeProvider: TimeProvider,
) : ExternalPaymentService {
    private companion object {
        /** How long a payment's transactions keep being rebuilt before it is given up. */
        val RETRY_WINDOW = 5.minutes
    }

    override suspend fun initiatePayment(
        key: ExternalPaymentKey,
        amount: Balance,
        destination: AccountId,
        confirmed: Boolean,
    ): Result<Unit> = core.units().flatMap { units ->
        val retry = Retry.Until((timeProvider.now() + RETRY_WINDOW).epochSeconds.toULong())
        core.use { coinage -> coinage.offload(key.coreId(), retry, units.centsOf(amount), destination.value, confirmed) }
    }.flatMap { started ->
        if (started is Started.New) Result.success(Unit) else Result.failure(ExternalPaymentError.AlreadyExists(key))
    }

    override suspend fun exists(key: ExternalPaymentKey): Result<Boolean> =
        core.use { coinage ->
            try {
                coinage.observeStatus(key.coreId(), IgnoredStatus).cancel()
                true
            } catch (_: UnknownOperation) {
                false
            }
        }

    override fun subscribePaymentStatus(key: ExternalPaymentKey): Flow<PaymentStatus> = flow {
        val units = core.units().getOrElse { throw it }
        val items = core.observe<StatusItem> { coinage, push ->
            coinage.observeStatus(
                key.coreId(),
                object : NativeStatusObserver {
                    override fun onStatus(item: StatusItem) = push(item)
                }
            )
        }
        emitAll(
            items.map { it.toPaymentStatus(units) }.transformWhile { status ->
                emit(status)
                status == PaymentStatus.Processing
            }
        )
    }.catch { error -> throw if (error is UnknownOperation) ExternalPaymentError.NotFound(key) else error }
}

private object IgnoredStatus : NativeStatusObserver {
    override fun onStatus(item: StatusItem) = Unit
}

private fun ExternalPaymentKey.coreId(): ByteArray = CoinageIds.of("external:$origin:$id")

private fun StatusItem.toPaymentStatus(units: CoinageUnits): PaymentStatus = when (val status = status) {
    Status.Done -> PaymentStatus.Completed
    Status.PartiallyDone -> PaymentStatus.PartiallyClaimed(units.balanceOf(progress.settled))
    is Status.Failed -> PaymentStatus.Failed(status.v1.name)
    else -> PaymentStatus.Processing
}
