package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.findRelayChainOrThrow
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.utilityAsset
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.memory.useCache
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.graph.SimpleEdge
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.CrossChainTransferService
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDirection
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDirectionId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOutcome
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFee
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferSuccess
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChain
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChainAsset
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.CrossChainTransfersRepository
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunner
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model.paidByAccountOrNull
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact.CrossChainTransactor
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact.CrossChainWeigher
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransferConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.availableDirectionIds
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.destinationChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.originChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.transferConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.transferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.TokenReserveRegistry
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.remoteReserveLocation
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleAccountFee
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleFee
import io.paritytech.polkadotapp.feature_xcm_api.config.XcmConfigRepository
import io.paritytech.polkadotapp.feature_xcm_api.config.model.GeneralXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.converter.LocationConverterFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.ZERO

internal class RealCrossChainTransferService @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val computationalCache: ComputationalCache,
    private val crossChainTransfersRepository: CrossChainTransfersRepository,
    private val xcmConfigRepository: XcmConfigRepository,
    private val locationConverterFactory: LocationConverterFactory,
    private val crossChainTransactor: CrossChainTransactor,
    private val assetTransferDryRunner: XcmTransferDryRunner,
    private val crossChainWeigher: CrossChainWeigher,
    private val chainStateRepository: ChainStateRepository,
    private val accountRepository: AccountRepository,
) : CrossChainTransferService {
    companion object {
        private const val CONFIGURATION_CACHE = "RealCrossChainTransferService.Configuration"
    }

    context(scope: ComputationalScope)
    override suspend fun availableDirectionIds(): List<CrossChainTransferDirectionId> {
        return cachedConfiguration().availableDirectionIds()
    }

    context(scope: ComputationalScope)
    override suspend fun getTransferFeatures(directionId: CrossChainTransferDirectionId): CrossChainTransferFeatures {
        val features = cachedConfiguration().transferFeatures(directionId)
        return requireNotNull(features) {
            "Transfer for $directionId was not found"
        }
    }

    override suspend fun getDirectionById(directionId: CrossChainTransferDirectionId): CrossChainTransferDirection {
        return SimpleEdge(
            from = chainRegistry.chainWithAsset(directionId.from),
            to = chainRegistry.chainWithAsset(directionId.to)
        )
    }

    context(scope: ComputationalScope)
    override suspend fun requiredRemainingAmountAfterTransfer(directionId: CrossChainTransferDirectionId): Balance {
        val transferConfig = getTransferConfigurationById(directionId)
        return crossChainTransactor.requiredRemainingAmountAfterTransfer(transferConfig)
    }

    context(scope: ComputationalScope)
    override suspend fun estimateMaximumExecutionTime(directionId: CrossChainTransferDirectionId): Duration {
        val transferConfig = getTransferConfigurationById(directionId)

        val originChainId = transferConfig.originChainId
        val remoteReserveChainId = transferConfig.transferType.remoteReserveLocation()?.chainId
        val destinationChainId = transferConfig.destinationChainId

        val relayId = chainRegistry.findRelayChainOrThrow(originChainId)

        var totalDuration = ZERO

        if (remoteReserveChainId != null) {
            totalDuration += maxTimeToTransmitMessage(originChainId, remoteReserveChainId, relayId)
            totalDuration += maxTimeToTransmitMessage(
                remoteReserveChainId,
                destinationChainId,
                relayId
            )
        } else {
            totalDuration += maxTimeToTransmitMessage(originChainId, destinationChainId, relayId)
        }

        return totalDuration
    }

    context(scope: ComputationalScope)
    override suspend fun estimateFee(
        transfer: CrossChainTransfer,
    ): Result<CrossChainTransferFee> = withContext(Dispatchers.IO) {
        val transferConfiguration = getTransferConfiguration(transfer.direction)

        val originFeeAsync = async {
            crossChainTransactor.estimateOriginFee(
                configuration = transferConfiguration,
                transfer = transfer,
                sender = accountRepository.getWalletAccount()
            )
        }
        val crossChainFeeAsync = async { crossChainWeigher.estimateFee(transferConfiguration, transfer) }

        val originFee = originFeeAsync.await()
        val crossChainFee = crossChainFeeAsync.await()

        originFee.flatMap { originFee ->
            crossChainFee.map { crossChainFee ->
                CrossChainTransferFee(
                    submissionFee = originFee,
                    postSubmissionByAccount = crossChainFee.paidByAccountOrNull()
                        ?.let { paidByAccount ->
                            SimpleAccountFee(
                                origin = originFee.origin,
                                amount = paidByAccount,
                                // By account xcm fees are paid in native asset
                                asset = transfer.originChain.utilityAsset
                            )
                        },
                    postSubmissionFromAmount = SimpleFee(
                        amount = crossChainFee.paidFromHolding,
                        asset = transfer.originChainAsset,
                    ),
                )
            }
        }
    }

    context(scope: ComputationalScope)
    override suspend fun performAndTrackTransfer(
        transfer: CrossChainTransfer,
        sender: MetaAccount
    ): Result<CrossChainTransferSuccess> {
        val transferConfiguration = getTransferConfiguration(transfer.direction)
        return crossChainTransactor.performAndTrackTransfer(transferConfiguration, transfer, sender)
    }

    context(scope: ComputationalScope)
    override suspend fun dryRunTransfer(
        transfer: CrossChainTransfer,
        dryRunOrigin: CrossChainTransferDryRunOrigin,
    ): Result<CrossChainTransferDryRunOutcome> {
        val transferConfiguration = getTransferConfiguration(transfer.direction)
        return assetTransferDryRunner.dryRunXcmTransfer(
            transferConfiguration,
            transfer,
            dryRunOrigin
        )
            .map { CrossChainTransferDryRunOutcome(it.destination.depositedAmount) }
    }

    private suspend fun maxTimeToTransmitMessage(
        from: ChainId,
        to: ChainId,
        relay: ChainId
    ): Duration {
        val toProduceBlockOnOrigin = chainStateRepository.expectedBlockTime(from)
        val toProduceBlockOnDestination = chainStateRepository.expectedBlockTime(to)
        val toProduceBlockOnRelay = if (from != relay && to != relay) chainStateRepository.expectedBlockTime(relay) else ZERO

        return toProduceBlockOnOrigin + toProduceBlockOnRelay + toProduceBlockOnDestination
    }

    context(scope: ComputationalScope)
    private suspend fun getTransferConfigurationById(directionId: CrossChainTransferDirectionId): CrossChainTransferConfiguration {
        val direction = getDirectionById(directionId)
        return getTransferConfiguration(direction)
    }

    context(scope: ComputationalScope)
    private suspend fun getTransferConfiguration(direction: CrossChainTransferDirection): CrossChainTransferConfiguration {
        return cachedConfiguration().transferConfiguration(direction)!!
    }

    context(scope: ComputationalScope)
    private suspend fun cachedConfiguration(): CrossChainTransfersConfiguration {
        return computationalCache.useCache(CONFIGURATION_CACHE) {
            val xcmConfig = xcmConfigRepository.awaitXcmConfig()

            val directionsConfig = crossChainTransfersRepository.getDirectionsConfiguration()
                .getOrThrow()

            val tokenReserveRegistry = createTokenReserveRegistry(xcmConfig)

            CrossChainTransfersConfiguration(
                parachainIds = xcmConfig.chains.parachainIds,
                reserveRegistry = tokenReserveRegistry,
                directions = directionsConfig
            )
        }
    }

    private suspend fun createTokenReserveRegistry(xcmConfig: GeneralXcmConfig): TokenReserveRegistry {
        val locationConverter = locationConverterFactory.createChainConverter()

        return TokenReserveRegistry(
            xcmConfig = xcmConfig.assets,
            chainLocationConverter = locationConverter,
            chainRegistry = chainRegistry
        )
    }
}
