package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.TransitionResult
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds

class EnsureVouchersPaymentState @AssistedInject constructor(
    @Assisted override val context: PaymentContext,
    private val coinageRecyclingUseCase: CoinageRecyclingUseCase,
    private val externalPaymentPlanner: ExternalPaymentPlanner,
    private val offboardFactory: OffboardVouchersPaymentState.Factory,
    private val awaitRecyclingFactory: AwaitRecyclingPaymentState.Factory,
) : ExternalPaymentState {
    companion object {
        const val INSUFFICIENT_BALANCE = "insufficient balance"
        const val RECYCLING_SUBMISSION_FAILED = "recycling submission failed"
    }

    override val id: String = "EnsureVouchers"

    @AssistedFactory
    interface Factory {
        fun create(context: PaymentContext): EnsureVouchersPaymentState
    }

    context(noContext: NoContext)
    override suspend fun performTransition(): TransitionResult<ExternalPaymentState> = transition {
        externalPaymentPlanner.plan(context.amount).flatMap { plan ->
            coinageLogD("External payment planned payment=${context.key} plan=$plan")

            nextStateFor(plan)
        }
    }

    private suspend fun nextStateFor(plan: ExternalPaymentPlan): Result<ExternalPaymentState> = when (plan) {
        is ExternalPaymentPlan.Ready -> Result.success(
            offboardFactory.create(
                context = context,
                selected = plan.offboarding.vouchers.map(RecyclerVoucher::ringVrfKeyIndex),
                surplusPlanks = plan.offboarding.surplus.value,
            )
        )

        is ExternalPaymentPlan.LoadCoins -> Result.success(recycle(plan))

        is ExternalPaymentPlan.NotEnoughAmount -> {
            coinageLogE("External payment has not enough funds payment=${context.key} amount=${context.amount} plan=$plan")

            Result.success(FailedPaymentState(context, INSUFFICIENT_BALANCE))
        }
    }

    private suspend fun recycle(plan: ExternalPaymentPlan.LoadCoins): ExternalPaymentState {
        val groupId = ExternalPaymentGroupIds.recycling(context.key)

        return coinageRecyclingUseCase.recycle(plan.coinsToLoad, groupId).fold(
            onSuccess = {
                awaitRecyclingFactory.create(
                    context = context,
                    exactVouchers = plan.exactVouchers.map(RecyclerVoucher::ringVrfKeyIndex),
                )
            },
            onFailure = { error ->
                coinageLogE("External payment recycling submission failed payment=${context.key}: ${error.message}")

                FailedPaymentState(context, RECYCLING_SUBMISSION_FAILED)
            }
        )
    }
}
