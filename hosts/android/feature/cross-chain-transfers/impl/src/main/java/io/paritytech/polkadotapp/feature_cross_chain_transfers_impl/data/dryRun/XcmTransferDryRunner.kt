package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findEvent
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.composeBatchAll
import io.paritytech.polkadotapp.chains.util.composeDispatchAs
import io.paritytech.polkadotapp.chains.util.emptyAccountId
import io.paritytech.polkadotapp.chains.util.isUtilityAsset
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.chains.util.utilityAsset
import io.paritytech.polkadotapp.chains.util.xcmPalletName
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.decimalWithdrawAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.destinationChainAsset
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChain
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChainAsset
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunResult.FinalSegment
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunResult.IntermediateSegment
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact.CrossChainTransactor
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransferConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.originChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.isRemoteReserve
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.remoteReserveLocation
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssets
import io.paritytech.polkadotapp.feature_xcm_api.asset.requireFungible
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.DryRunEffects
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.getByLocation
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.senderXcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.getInnerSuccessOrThrow
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.versionedXcm
import javax.inject.Inject

internal interface XcmTransferDryRunner {
    suspend fun dryRunXcmTransfer(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        origin: CrossChainTransferDryRunOrigin
    ): Result<XcmTransferDryRunResult>
}

internal class RealXcmTransferDryRunner @Inject constructor(
    private val dryRunApi: DryRunApi,
    private val chainRegistry: ChainRegistry,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val crossChainTransactor: CrossChainTransactor,
) : XcmTransferDryRunner {
    companion object {
        private const val MINIMUM_FUND_AMOUNT = 100

        private const val FEES_PAID_FEES_ARGUMENT_INDEX = 1
        private const val ASSETS_TRAPPED_ARGUMENT_INDEX = 2
    }

    override suspend fun dryRunXcmTransfer(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        origin: CrossChainTransferDryRunOrigin
    ): Result<XcmTransferDryRunResult> {
        return runCatching {
            val originResult = dryRunOnOrigin(config, transfer, origin)
            val remoteReserveResult = dryRunOnRemoteReserve(config, originResult.forwardedXcm)
            val destinationResult = dryRunOnDestination(config, transfer, remoteReserveResult.forwardedXcm)

            XcmTransferDryRunResult(
                origin = originResult.toPublicResult(),
                remoteReserve = remoteReserveResult.takeIfRemoteReserve(config)?.toPublicResult(),
                destination = destinationResult.toPublicResult()
            )
        }
            .logFailure("Dry run failed")
    }

    private suspend fun dryRunOnOrigin(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        origin: CrossChainTransferDryRunOrigin,
    ): IntermediateDryRunResult {
        val xcmResultsVersion = XcmVersion.V4

        val (dryRunCall, dryRunOrigin) = chainRegistry.withRuntime(config.originChainId) {
            constructDryRunCallParams(config, transfer, origin)
        }
        val dryRunResult = dryRunApi.dryRunCall(dryRunOrigin, dryRunCall, config.originChainId, xcmResultsVersion)
            .getInnerSuccessOrThrow()

        val nextHopLocation = (config.transferType.remoteReserveLocation() ?: config.destinationChainLocation).location

        val forwardedXcm = searchForwardedXcm(
            dryRunEffects = dryRunResult,
            destination = nextHopLocation.fromPointOfViewOf(config.originChainLocation.location),
        )

        return chainRegistry.withRuntime(config.originChainId) {
            val deliveryFee = searchDeliveryFee(dryRunResult)
            val trappedAssets = searchTrappedAssets(dryRunResult)

            IntermediateDryRunResult(forwardedXcm, deliveryFee, trappedAssets)
        }
    }

    context(withRuntime: WithRuntime)
    private suspend fun constructDryRunCallParams(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        origin: CrossChainTransferDryRunOrigin,
    ): OriginCallParams {
        return when (origin) {
            CrossChainTransferDryRunOrigin.Fake -> constructDryRunCallFromFakeOrigin(transfer, config)

            is CrossChainTransferDryRunOrigin.Signed -> constructDryRunCallFromRealOrigin(transfer, config, origin)
        }
    }

    private suspend fun constructDryRunCallFromRealOrigin(
        transfer: CrossChainTransfer,
        config: CrossChainTransferConfiguration,
        origin: CrossChainTransferDryRunOrigin.Signed,
    ): OriginCallParams {
        val callOnOrigin = crossChainTransactor.composeCrossChainTransferCall(config, transfer)

        return OriginCallParams(
            call = callOnOrigin,
            origin = OriginCaller.System.Signed(origin.accountId)
        )
    }

    context(withRuntime: WithRuntime)
    private suspend fun constructDryRunCallFromFakeOrigin(
        transfer: CrossChainTransfer,
        config: CrossChainTransferConfiguration,
    ): OriginCallParams {
        val callOnOrigin = crossChainTransactor.composeCrossChainTransferCall(config, transfer)

        val dryRunAccount = transfer.originChain.emptyAccountId()
        val transferOrigin = OriginCaller.System.Signed(dryRunAccount)

        val calls = buildList {
            addFundCalls(transfer, dryRunAccount)

            val transferCallFromOrigin = composeDispatchAs(callOnOrigin, transferOrigin)
            add(transferCallFromOrigin)
        }

        val finalOriginCall = composeBatchAll(calls)
        return OriginCallParams(finalOriginCall, OriginCaller.System.Root)
    }

    private suspend fun MutableList<GenericCall.Instance>.addFundCalls(transfer: CrossChainTransfer, dryRunAccount: AccountId) {
        val fundAmount = determineFundAmount(transfer)

        // Fund native asset first so we can later fund potentially non-sufficient assets
        if (!transfer.originChainAsset.isUtilityAsset) {
            // Additionally fund native asset to pay delivery fees
            val nativeAsset = transfer.originChain.utilityAsset
            val planks = nativeAsset.planksFromAmount(MINIMUM_FUND_AMOUNT.toBigDecimal())
            val fundNativeAssetCall = tokenBalanceTypeRegistry.issuerFor(nativeAsset).composeIssueCall(planks, dryRunAccount)
            add(fundNativeAssetCall)
        }

        val fundSendingAssetCall = tokenBalanceTypeRegistry.issuerFor(transfer.originChainAsset).composeIssueCall(fundAmount, dryRunAccount)
        add(fundSendingAssetCall)
    }

    private suspend fun dryRunOnRemoteReserve(
        config: CrossChainTransferConfiguration,
        forwardedFromOrigin: VersionedRawXcmMessage,
    ): IntermediateDryRunResult {
        // No remote reserve - nothing to dry run, return unchanged value
        val remoteReserveLocation = config.transferType.remoteReserveLocation()
            ?: return IntermediateDryRunResult(forwardedFromOrigin, Balance.ZERO, Balance.ZERO)

        val originLocation = config.originChainLocation.location
        val destinationLocation = config.destinationChainLocation.location

        val usedXcmVersion = forwardedFromOrigin.version

        val dryRunOrigin = originLocation.fromPointOfViewOf(remoteReserveLocation.location).versionedXcm(usedXcmVersion)
        val dryRunResult = dryRunApi.dryRunXcm(dryRunOrigin, forwardedFromOrigin, remoteReserveLocation.chainId)
            .getInnerSuccessOrThrow()

        val destinationOnRemoteReserve = destinationLocation.fromPointOfViewOf(remoteReserveLocation.location)

        val forwardedXcm = searchForwardedXcm(
            dryRunEffects = dryRunResult,
            destination = destinationOnRemoteReserve,
        )

        return chainRegistry.withRuntime(remoteReserveLocation.chainId) {
            val deliveryFee = searchDeliveryFee(dryRunResult)
            val trappedAssets = searchTrappedAssets(dryRunResult)

            IntermediateDryRunResult(forwardedXcm, deliveryFee, trappedAssets)
        }
    }

    private suspend fun dryRunOnDestination(
        config: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        forwardedFromPrevious: VersionedRawXcmMessage,
    ): FinalDryRunResult {
        val previousLocation = (config.transferType.remoteReserveLocation() ?: config.originChainLocation).location
        val destinationLocation = config.destinationChainLocation

        val usedXcmVersion = forwardedFromPrevious.version

        val dryRunOrigin = previousLocation.fromPointOfViewOf(destinationLocation.location).versionedXcm(usedXcmVersion)
        val dryRunResult = dryRunApi.dryRunXcm(dryRunOrigin, forwardedFromPrevious, destinationLocation.chainId)
            .getInnerSuccessOrThrow()

        val depositedAmount = searchDepositAmount(dryRunResult, transfer.destinationChainAsset, transfer.recipient)

        return FinalDryRunResult(depositedAmount)
    }

    private fun searchForwardedXcm(
        dryRunEffects: DryRunEffects,
        destination: RelativeMultiLocation,
    ): VersionedRawXcmMessage {
        return searchForwardedXcmInQueues(dryRunEffects, destination)
    }

    private suspend fun searchDepositAmount(
        dryRunEffects: DryRunEffects,
        chainAsset: Chain.Asset,
        recipientAccountId: AccountId,
    ): Balance {
        val depositDetector = tokenBalanceTypeRegistry.eventDetectorFor(chainAsset)

        val deposits = dryRunEffects.emittedEvents.mapNotNull { depositDetector.detectDeposit(it) }
            .filter { it.destination == recipientAccountId }

        if (deposits.isEmpty()) error("No deposits detected")

        return deposits.sumOf { it.amount.value }.intoBalance()
    }

    context(withRuntime: WithRuntime)
    private fun searchDeliveryFee(
        dryRunEffects: DryRunEffects,
    ): Balance {
        val xcmPalletName = withRuntime.runtime.metadata.xcmPalletName()
        val event = dryRunEffects.emittedEvents.findEvent(xcmPalletName, "FeesPaid") ?: return Balance.ZERO

        val usedXcmVersion = dryRunEffects.senderXcmVersion()

        val feesDecoded = event.arguments[FEES_PAID_FEES_ARGUMENT_INDEX]
        val multiAssets = MultiAssets.bind(feesDecoded, usedXcmVersion)

        return multiAssets.extractFirstAmount()
    }

    context(withRuntime: WithRuntime)
    private fun searchTrappedAssets(
        dryRunEffects: DryRunEffects,
    ): Balance {
        val xcmPalletName = withRuntime.runtime.metadata.xcmPalletName()
        val event = dryRunEffects.emittedEvents.findEvent(xcmPalletName, "AssetsTrapped") ?: return Balance.ZERO

        val feesDecoded = event.arguments[ASSETS_TRAPPED_ARGUMENT_INDEX]
        val multiAssets = MultiAssets.bindVersioned(feesDecoded).xcm

        return multiAssets.extractFirstAmount()
    }

    private fun MultiAssets.extractFirstAmount(): Balance {
        return if (value.isNotEmpty()) {
            value.first().requireFungible().amount
        } else {
            Balance.ZERO
        }
    }

    private fun searchForwardedXcmInQueues(
        dryRunEffects: DryRunEffects,
        destination: RelativeMultiLocation
    ): VersionedRawXcmMessage {
        val usedXcmVersion = dryRunEffects.senderXcmVersion()
        val versionedDestination = destination.versionedXcm(usedXcmVersion)

        val forwardedXcmsToDestination = dryRunEffects.forwardedXcms.getByLocation(versionedDestination)

        // There should only be one forwarded message during dry run
        return forwardedXcmsToDestination.first()
    }

    private fun determineFundAmount(transfer: CrossChainTransfer): Balance {
        val amount = (transfer.decimalWithdrawAmount() * 2.toBigDecimal()).coerceAtLeast(MINIMUM_FUND_AMOUNT.toBigDecimal())
        return transfer.originChainAsset.planksFromAmount(amount)
    }

    private fun IntermediateDryRunResult.toPublicResult(): IntermediateSegment {
        return IntermediateSegment(
            deliveryFee = deliveryFee,
            trapped = trapped
        )
    }

    private fun FinalDryRunResult.toPublicResult(): FinalSegment {
        return FinalSegment(depositedAmount = depositedAmount)
    }

    private fun IntermediateDryRunResult.takeIfRemoteReserve(config: CrossChainTransferConfiguration): IntermediateDryRunResult? {
        return takeIf { config.transferType.isRemoteReserve() }
    }

    private class IntermediateDryRunResult(
        val forwardedXcm: VersionedRawXcmMessage,
        val deliveryFee: Balance,
        val trapped: Balance,
    )

    private class FinalDryRunResult(
        val depositedAmount: Balance
    )

    private data class OriginCallParams(val call: GenericCall.Instance, val origin: OriginCaller)
}
