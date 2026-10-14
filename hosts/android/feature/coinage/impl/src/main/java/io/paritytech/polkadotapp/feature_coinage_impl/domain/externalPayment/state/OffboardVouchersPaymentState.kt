package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.TransitionResult
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase.ExternalUnloadStatus
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase.UnloadRecyclerIntoExternalAssetUseCase
import kotlinx.coroutines.flow.first
import java.math.BigInteger

/**
 * Offboards a selected subset of recycler vouchers straight into the destination account's
 * external-asset balance. When `surplusPlanks` is positive, the surplus is folded back into
 * freshly-minted vouchers inside the same call via
 * Coinage.unload_recycler_into_external_asset_and_loaded_coins; otherwise
 * Coinage.unload_recycler_into_external_asset is used.
 */
class OffboardVouchersPaymentState @AssistedInject constructor(
    @Assisted override val context: PaymentContext,
    @Assisted val selected: List<CoinageKeyIndex>,
    @Assisted val surplusPlanks: BigInteger,
    private val voucherRepository: VoucherRepository,
    private val unloadIntoExternalAsset: UnloadRecyclerIntoExternalAssetUseCase,
) : ExternalPaymentState {
    companion object {
        const val UNLOAD_SUBMISSION_FAILED = "unload submission failed"
        const val NOTHING_UNLOADED = "no unload transaction executed"
    }

    override val id: String = "OffboardVouchers"

    val surplus: Balance get() = surplusPlanks.intoBalance()

    @AssistedFactory
    interface Factory {
        fun create(
            context: PaymentContext,
            selected: List<CoinageKeyIndex>,
            surplusPlanks: BigInteger,
        ): OffboardVouchersPaymentState
    }

    context(noContext: NoContext)
    override suspend fun performTransition(): TransitionResult<ExternalPaymentState> = transition {
        val groupId = ExternalPaymentGroupIds.unload(context.key)

        unloadIntoExternalAsset.initiateUnload(
            vouchers = voucherRepository.getByRingVrfKeyIndices(selected),
            destination = context.destination,
            surplus = surplus,
            groupId = groupId,
        ).fold(
            onSuccess = { Result.success(awaitOutcome(groupId)) },
            onFailure = { error ->
                coinageLogE("External payment unload submission failed payment=${context.key}: ${error.message}")

                Result.success(FailedPaymentState(context, UNLOAD_SUBMISSION_FAILED))
            }
        )
    }

    private suspend fun awaitOutcome(groupId: CoinageOperationGroupId): ExternalPaymentState {
        val outcome = unloadIntoExternalAsset.subscribeUnloadStatus(groupId)
            .first { it !is ExternalUnloadStatus.Submitted }

        return when (outcome) {
            is ExternalUnloadStatus.FinalizedSuccess -> CompletedPaymentState(context)

            is ExternalUnloadStatus.PartialSuccess -> PartiallyCompletedPaymentState(context, outcome.claimed)

            is ExternalUnloadStatus.Failed, is ExternalUnloadStatus.Submitted -> FailedPaymentState(context, NOTHING_UNLOADED)
        }
    }
}
