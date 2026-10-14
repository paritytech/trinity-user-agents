package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.BlockEvents
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.EventsRepository
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.hasEvent
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.transformResult
import io.paritytech.polkadotapp.common.utils.wrapIntoResult
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.tryDetectDeposit
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferSuccess
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.destinationChain
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.destinationChainAsset
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChain
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.withdrawAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransferConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.originChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.XcmTransferType
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetFilter
import io.paritytech.polkadotapp.feature_xcm_api.builder.XcmBuilder
import io.paritytech.polkadotapp.feature_xcm_api.builder.buyExecution
import io.paritytech.polkadotapp.feature_xcm_api.builder.createWithoutFeesMeasurement
import io.paritytech.polkadotapp.feature_xcm_api.builder.withdrawAsset
import io.paritytech.polkadotapp.feature_xcm_api.extrinsic.composeXcmExecute
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.getInnerSuccessOrThrow
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment.XcmPaymentApi
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.weight.WeightLimit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.withTimeout
import timber.log.Timber
import javax.inject.Inject
import kotlin.coroutines.coroutineContext
import kotlin.time.Duration.Companion.seconds

private val USED_XCM_VERSION = XcmVersion.V4

internal class CrossChainTransactor @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val xcmBuilderFactory: XcmBuilder.Factory,
    private val xcmPaymentApi: XcmPaymentApi,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val usingTypeTransactor: TransferAssetUsingTypeTransactor,
    private val extrinsicService: ExtrinsicService,
    private val eventsRepository: EventsRepository,
    private val signedOrigins: SignedOrigins,
) {
    suspend fun requiredRemainingAmountAfterTransfer(
        configuration: CrossChainTransferConfiguration
    ): Balance {
        return if (supportsXcmExecute(configuration)) {
            Balance.ZERO
        } else {
            val chainAsset = configuration.originChainAsset
            tokenBalanceTypeRegistry.typeFor(chainAsset).minimumBalance()
        }
    }

    suspend fun composeCrossChainTransferCall(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
    ): GenericCall.Instance {
        return if (supportsXcmExecute(configuration)) {
            composeXcmExecuteCall(configuration, transfer)
        } else {
            usingTypeTransactor.composeCall(
                configuration,
                transfer,
                forceXcmVersion = USED_XCM_VERSION
            )
        }
    }

    suspend fun supportsXcmExecute(
        originChainId: ChainId,
        features: CrossChainTransferFeatures
    ): Boolean {
        val supportsXcmExecute = features.supportsXcmExecute
        val hasXcmPaymentApi = xcmPaymentApi.isSupported(originChainId)

        // For now, only enable xcm execute approach for the directions that will hugely benefit from it
        // In particular, xcm execute allows us to pay delivery fee from the holding register and not in JIT mode (from account)
        val hasDeliveryFee = features.hasDeliveryFees

        return supportsXcmExecute && hasXcmPaymentApi && hasDeliveryFee
    }

    suspend fun estimateOriginFee(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        sender: MetaAccount,
    ): Result<AccountFee> {
        return extrinsicService.estimateFee(
            chain = transfer.originChain,
            origin = signedOrigins.signedOrigin(sender),
            options = ExtrinsicService.SubmissionOptions(
                feePayment = transfer.originFeePayment
            )
        ) {
            val call = composeCrossChainTransferCall(configuration, transfer)
            call(call)
        }
    }

    suspend fun performAndTrackTransfer(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        sender: MetaAccount
    ): Result<CrossChainTransferSuccess> {
        // Start balances updates eagerly to not to miss events in case tx has been included to block right after submission
        val balancesUpdates = observeTransferableBalance(transfer)
            .wrapIntoResult()
            .shareIn(CoroutineScope(coroutineContext), SharingStarted.Eagerly, replay = 100)

        Timber.d("Starting cross-chain transfer")

        return performTransfer(configuration, transfer, sender)
            .flattenExecutionFailure()
            .flatMap {
                Timber.d("Cross chain transfer for successfully executed on origin, waiting for destination")

                balancesUpdates.awaitCrossChainArrival(transfer)
            }
    }

    private suspend fun performTransfer(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        sender: MetaAccount
    ): Result<ExtrinsicExecutionResult> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(
            chain = transfer.originChain,
            origin = signedOrigins.signedOrigin(sender),
            options = ExtrinsicService.SubmissionOptions(
                feePayment = transfer.originFeePayment
            )
        ) {
            val call = composeCrossChainTransferCall(configuration, transfer)
            call(call)
        }
    }

    private suspend fun Flow<Result<AccountBalanceUpdate>>.awaitCrossChainArrival(transfer: CrossChainTransfer): Result<CrossChainTransferSuccess> {
        return runCatching {
            withTimeout(60.seconds) {
                transformResult { balanceUpdate ->
                    Timber.d("Destination balance update detected: $balanceUpdate")

                    val updatedAt = balanceUpdate.updatedAt ?: return@transformResult

                    val blockEvents = eventsRepository.getEventsInBlock(transfer.destinationChain.id, updatedAt)

                    val xcmArrivedDeposit = searchForXcmArrival(blockEvents.initialization, transfer)
                        ?: searchForXcmArrival(blockEvents.finalization, transfer)
                        ?: searchForXcmArrival(
                            blockEvents.findSetValidationDataEvents(),
                            transfer
                        )

                    if (xcmArrivedDeposit != null) {
                        Timber.d("Found destination xcm arrival event, amount is $xcmArrivedDeposit")

                        emit(CrossChainTransferSuccess(xcmArrivedDeposit))
                    } else {
                        Timber.d("No destination xcm arrival event found for the received balance update")
                    }
                }
                    .first()
                    .getOrThrow()
            }
        }
    }

    private fun BlockEvents.findSetValidationDataEvents(): List<GenericEvent.Instance> {
        val setValidationDataExtrinsic = applyExtrinsic.find {
            it.extrinsic.call.instanceOf(Modules.PARACHAIN_SYSTEM, "set_validation_data")
        }

        return setValidationDataExtrinsic?.events.orEmpty()
    }

    private suspend fun searchForXcmArrival(
        events: List<GenericEvent.Instance>,
        transfer: CrossChainTransfer
    ): Balance? {
        if (!events.hasXcmArrivalEvent()) return null

        val eventDetector = tokenBalanceTypeRegistry.eventDetectorFor(transfer.destinationChainAsset)

        val depositEvent = events.mapNotNull { event -> eventDetector.tryDetectDeposit(event) }
            .find { it.destination == transfer.recipient }

        return depositEvent?.amount
    }

    private fun List<GenericEvent.Instance>.hasXcmArrivalEvent(): Boolean {
        return hasEvent("MessageQueue", "Processed") or hasEvent("XcmpQueue", "Success")
    }

    private suspend fun observeTransferableBalance(transfer: CrossChainTransfer): Flow<AccountBalanceUpdate> {
        val destinationAssetBalances =
            tokenBalanceTypeRegistry.typeFor(transfer.destinationChainAsset)

        return destinationAssetBalances.subscribeAccountBalanceUpdates(accountId = transfer.recipient)
    }

    private suspend fun supportsXcmExecute(configuration: CrossChainTransferConfiguration): Boolean {
        return supportsXcmExecute(configuration.originChainId, configuration.features)
    }

    private suspend fun composeXcmExecuteCall(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
    ): GenericCall.Instance {
        val xcmProgram = buildXcmProgram(configuration, transfer)
        val weight = xcmPaymentApi.queryXcmWeight(configuration.originChainId, xcmProgram)
            .getInnerSuccessOrThrow()

        return chainRegistry.withRuntime(configuration.originChainId) {
            composeXcmExecute(xcmProgram, weight)
        }
    }

    private suspend fun buildXcmProgram(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
    ): VersionedXcmMessage {
        val builder = xcmBuilderFactory.createWithoutFeesMeasurement(
            initial = configuration.originChainLocation,
            xcmVersion = USED_XCM_VERSION
        )

        builder.buildTransferProgram(configuration, transfer)

        return builder.build()
    }

    private fun XcmBuilder.buildTransferProgram(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
    ) {
        val assetAbsoluteMultiLocation = configuration.transferType.assetAbsoluteLocation

        when (val transferType = configuration.transferType) {
            is XcmTransferType.Teleport -> buildTeleportProgram(
                assetLocation = assetAbsoluteMultiLocation,
                destinationChainLocation = configuration.destinationChainLocation,
                beneficiary = transfer.recipient,
                amount = transfer.withdrawAmount
            )

            is XcmTransferType.Reserve.Origin -> buildOriginReserveProgram(
                assetLocation = assetAbsoluteMultiLocation,
                destinationChainLocation = configuration.destinationChainLocation,
                beneficiary = transfer.recipient,
                amount = transfer.withdrawAmount
            )

            is XcmTransferType.Reserve.Destination -> buildDestinationReserveProgram(
                assetLocation = assetAbsoluteMultiLocation,
                destinationChainLocation = configuration.destinationChainLocation,
                beneficiary = transfer.recipient,
                amount = transfer.withdrawAmount
            )

            is XcmTransferType.Reserve.Remote -> buildRemoteReserveProgram(
                assetLocation = assetAbsoluteMultiLocation,
                remoteReserveLocation = transferType.remoteReserveLocation,
                destinationChainLocation = configuration.destinationChainLocation,
                beneficiary = transfer.recipient,
                amount = transfer.withdrawAmount
            )
        }
    }

    private fun XcmBuilder.buildTeleportProgram(
        assetLocation: AbsoluteMultiLocation,
        destinationChainLocation: ChainLocation,
        beneficiary: AccountId,
        amount: Balance,
    ) {
        val feesAmount = deriveBuyExecutionUpperBoundAmount(amount)

        // Origin
        withdrawAsset(assetLocation, amount)
        // Here and onward: we use buy execution for the very first segment to be able to pay delivery fees in sending asset
        // WeightLimit.one() is used since it doesn't matter anyways as the message on origin is already weighted
        // The only restriction is that it cannot be zero or Unlimited
        buyExecution(assetLocation, feesAmount, WeightLimit.one())
        initiateTeleport(MultiAssetFilter.singleCounted(), destinationChainLocation)

        // Destination
        buyExecution(assetLocation, feesAmount, WeightLimit.Unlimited)
        depositAsset(MultiAssetFilter.singleCounted(), beneficiary)
    }

    private fun XcmBuilder.buildOriginReserveProgram(
        assetLocation: AbsoluteMultiLocation,
        destinationChainLocation: ChainLocation,
        beneficiary: AccountId,
        amount: Balance,
    ) {
        val feesAmount = deriveBuyExecutionUpperBoundAmount(amount)

        // Origin
        withdrawAsset(assetLocation, amount)
        buyExecution(assetLocation, feesAmount, WeightLimit.one())
        depositReserveAsset(MultiAssetFilter.singleCounted(), destinationChainLocation)

        // Destination
        buyExecution(assetLocation, feesAmount, WeightLimit.Unlimited)
        depositAsset(MultiAssetFilter.singleCounted(), beneficiary)
    }

    private fun XcmBuilder.buildDestinationReserveProgram(
        assetLocation: AbsoluteMultiLocation,
        destinationChainLocation: ChainLocation,
        beneficiary: AccountId,
        amount: Balance,
    ) {
        val feesAmount = deriveBuyExecutionUpperBoundAmount(amount)

        // Origin
        withdrawAsset(assetLocation, amount)
        buyExecution(assetLocation, feesAmount, WeightLimit.one())
        initiateReserveWithdraw(MultiAssetFilter.singleCounted(), destinationChainLocation)

        // Destination
        buyExecution(assetLocation, feesAmount, WeightLimit.Unlimited)
        depositAsset(MultiAssetFilter.singleCounted(), beneficiary)
    }

    private fun XcmBuilder.buildRemoteReserveProgram(
        assetLocation: AbsoluteMultiLocation,
        remoteReserveLocation: ChainLocation,
        destinationChainLocation: ChainLocation,
        beneficiary: AccountId,
        amount: Balance,
    ) {
        val feesAmount = deriveBuyExecutionUpperBoundAmount(amount)

        // Origin
        withdrawAsset(assetLocation, amount)
        buyExecution(assetLocation, feesAmount, WeightLimit.one())
        initiateReserveWithdraw(MultiAssetFilter.singleCounted(), remoteReserveLocation)

        // Remote reserve
        buyExecution(assetLocation, feesAmount, WeightLimit.Unlimited)
        depositReserveAsset(MultiAssetFilter.singleCounted(), destinationChainLocation)

        // Destination
        buyExecution(assetLocation, feesAmount, WeightLimit.Unlimited)
        depositAsset(MultiAssetFilter.singleCounted(), beneficiary)
    }

    private fun deriveBuyExecutionUpperBoundAmount(transferringAmount: Balance): Balance {
        return transferringAmount / 2
    }

    private fun WeightLimit.Companion.one(): WeightLimit.Limited {
        return WeightLimit.Limited(WeightV2(1, 1))
    }
}
