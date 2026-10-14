package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.TransitionResult
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.totalBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds
import kotlinx.coroutines.flow.first

/**
 * Waits for the coins [EnsureVouchersPaymentState] sent to recycling, then offboards what they became together
 * with [exactVouchers]. Best-block recycling is enough to go on: a reorg that undoes it fails the unload rather
 * than paying twice.
 */
class AwaitRecyclingPaymentState @AssistedInject constructor(
    @Assisted override val context: PaymentContext,
    @Assisted val exactVouchers: List<CoinageKeyIndex>,
    private val coinageRecyclingUseCase: CoinageRecyclingUseCase,
    private val voucherRepository: VoucherRepository,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
    private val externalPaymentPlanner: ExternalPaymentPlanner,
    private val offboardFactory: OffboardVouchersPaymentState.Factory,
) : ExternalPaymentState {
    companion object {
        const val RECYCLING_INCOMPLETE = "recycling incomplete"
        const val INSUFFICIENT_AFTER_RECYCLING = "insufficient balance after recycling"
    }

    override val id: String = "AwaitRecycling"

    @AssistedFactory
    interface Factory {
        fun create(
            context: PaymentContext,
            exactVouchers: List<CoinageKeyIndex>,
        ): AwaitRecyclingPaymentState
    }

    context(noContext: NoContext)
    override suspend fun performTransition(): TransitionResult<ExternalPaymentState> = transition {
        val status = coinageRecyclingUseCase.observeRecyclingStatus(ExternalPaymentGroupIds.recycling(context.key))
            .first { it is RecyclingStatus.AllRecycled || it is RecyclingStatus.Incomplete }

        when (status) {
            is RecyclingStatus.AllRecycled -> offboard(status.vouchers)

            else -> {
                coinageLogE("External payment recycling incomplete payment=${context.key}")

                Result.success(FailedPaymentState(context, RECYCLING_INCOMPLETE))
            }
        }
    }

    private suspend fun offboard(recycled: List<RecyclerVoucher>): Result<ExternalPaymentState> {
        val available = recycled + voucherRepository.getByRingVrfKeyIndices(exactVouchers)

        return coinageBalanceConverterUseCase.create().flatMap { converter ->
            val total = with(converter) { available.totalBalance() }
            coinageLogD("External payment recycled payment=${context.key} available=$total amount=${context.amount}")

            if (total < context.amount) {
                Result.success(FailedPaymentState(context, INSUFFICIENT_AFTER_RECYCLING))
            } else {
                externalPaymentPlanner.pickOffboarding(available, context.amount).map { offboarding ->
                    offboardFactory.create(
                        context = context,
                        selected = offboarding.vouchers.map(RecyclerVoucher::ringVrfKeyIndex),
                        surplusPlanks = offboarding.surplus.value,
                    )
                }
            }
        }
    }
}
