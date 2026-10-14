package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.orZero
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.destinationChainAsset
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.withdrawAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunResult
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunner
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model.CrossChainWeightResult
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransferConfiguration
import javax.inject.Inject

private const val MINIMUM_SEND_AMOUNT = 100

internal class CrossChainWeigher @Inject constructor(
    private val xcmTransferDryRunner: XcmTransferDryRunner,
) {
    suspend fun estimateFee(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer
    ): Result<CrossChainWeightResult> {
        val safeTransfer = transfer.ensureSafeAmount()
        return xcmTransferDryRunner.dryRunXcmTransfer(config, safeTransfer, CrossChainTransferDryRunOrigin.Fake)
            .mapCatching {
                CrossChainWeightResult.fromDryRunResult(
                    initialAmount = safeTransfer.withdrawAmount,
                    transferDryRunResult = it
                )
            }
    }

    // Ensure we can calculate fee regardless of what user entered
    private fun CrossChainTransfer.ensureSafeAmount(): CrossChainTransfer {
        val minimumSendAmount = destinationChainAsset.planksFromAmount(MINIMUM_SEND_AMOUNT.toBigDecimal())
        val safeAmount = userSpecifiedAmount.coerceAtLeast(minimumSendAmount)
        return copy(userSpecifiedAmount = safeAmount)
    }

    private fun CrossChainWeightResult.Companion.fromDryRunResult(
        initialAmount: Balance,
        transferDryRunResult: XcmTransferDryRunResult
    ): CrossChainWeightResult {
        return with(transferDryRunResult) {
            // We do not add `remoteReserve.deliveryFee` since it is paid from holding and not by account
            val paidByAccount = origin.deliveryFee

            val trapped = origin.trapped + remoteReserve?.trapped.orZero()
            val totalFee = initialAmount - destination.depositedAmount - trapped

            // We do not subtract `origin.deliveryFee` since it is paid directly from the origin account and thus do not contribute towards execution fee
            // We do not subtract `remoteReserve.deliveryFee` since it is paid from holding and thus is already accounted in totalFee
            val executionFee = totalFee

            CrossChainWeightResult(paidByAccount = paidByAccount, paidFromHolding = executionFee)
        }
    }
}
