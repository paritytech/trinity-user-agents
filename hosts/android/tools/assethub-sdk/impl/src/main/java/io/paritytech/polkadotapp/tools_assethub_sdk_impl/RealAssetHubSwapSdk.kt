package io.paritytech.polkadotapp.tools_assethub_sdk_impl

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findEventOrThrow
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.emptyAccountId
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.domain.model.toOriginCaller
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FastLookupCustomFeeCapability
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.metaAccountOrThrow
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.encodableMultiLocationOf
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.flatten
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.tools_assethub_sdk_api.AssetHubSdk
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSdkOverridableData
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSwapEdge
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.AssetHubDryRunOutcome
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapLimit
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapOutcome
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.AssetConversionQuoter
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api.Tuple2
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api.assetConversionOrNull
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api.pools
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.detectAssetIdXcmVersion
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.fee.AssetConversionFastLookupFeeCapability
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.fee.AssetConversionFeePayment
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map

class RealAssetHubSwapSdk(
    override val chain: Chain,
    private val chainRegistry: ChainRegistry,
    private val remoteStorageSource: StorageDataSource,
    private val multiLocationConverter: ChainAssetLocationConverter,
    private val overridableData: AssetHubSdkOverridableData,
    private val extrinsicService: ExtrinsicService,
    private val xcmVersionDetector: XcmVersionDetector,
    private val quoter: AssetConversionQuoter,
    private val feePaymentFactory: AssetConversionFeePayment.Factory,
    private val dryRunApi: DryRunApi,
    private val signedOrigins: SignedOrigins,
) : AssetHubSdk {
    private val availableSwapDirectionsCache = SingleValueCache {
        remoteStorageSource.queryCatching(chain.id) {
            val allPools = metadata.assetConversionOrNull?.pools?.keys().orEmpty()

            constructAllAvailableDirections(allPools)
        }
    }

    override suspend fun availableSwapDirections(): Collection<AssetHubSwapEdge> {
        return availableSwapDirectionsCache.invoke()
            .logFailure("Failed to get AssetConversion swap directions")
            .getOrEmpty()
    }

    override fun quoteInvalidationFlow(): Flow<Unit> {
        return flowOfAll { overridableData.blockNumber(chain.id) }
            .drop(1) // skip immediate value from the cache to not perform double-quote on chain change
            .map { }
    }

    override suspend fun performSwap(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
        origin: TransactionOrigin,
        recipient: AccountId,
    ): Result<SwapOutcome> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(
            chain = chain,
            origin = origin,
            options = ExtrinsicService.SubmissionOptions(
                feePayment = getFeePayment(feeAsset)
            )
        ) {
            executeSwap(
                swapLimit = swapLimit,
                sendTo = recipient,
                trade = trade
            )
        }
            .flattenExecutionFailure()
            .map { it.emittedEvents.determineSwapOutcome() }
    }

    override suspend fun dryRun(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        origin: TransactionOrigin,
        recipient: AccountId
    ): Result<AssetHubDryRunOutcome> {
        return chainRegistry.withRuntime(chain.id) {
            dryRunApi.dryRunCall(
                originCaller = origin.metaAccountOrThrow().toOriginCaller(chain),
                call = executeSwapCall(trade, swapLimit, recipient),
                chainId = chain.id,
            )
                .flatten()
                .map { dryRunEffects ->
                    val depositedAmount = dryRunEffects.emittedEvents.determineSwapOutcome()
                    AssetHubDryRunOutcome(depositedAmount.actualReceivedAmount)
                }
        }
    }

    override suspend fun estimateFee(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset
    ): Result<AccountFee> {
        return extrinsicService.estimateFee(
            chain = chain,
            options = ExtrinsicService.SubmissionOptions(
                feePayment = getFeePayment(feeAsset),
            ),
            origin = signedOrigins.wallet()
        ) {
            executeSwap(
                swapLimit = swapLimit,
                sendTo = chain.emptyAccountId(),
                trade = trade
            )
        }
    }

    override suspend fun getFeePayment(feeAsset: Chain.Asset): FeePayment {
        return feePaymentFactory.create(
            feePaymentAsset = feeAsset,
            quoter = quoter,
            multiLocationConverter = multiLocationConverter
        )
    }

    override suspend fun feeCapabilityLookup(chainId: ChainId): Result<FastLookupCustomFeeCapability> {
        return availableSwapDirectionsCache().map(::AssetConversionFastLookupFeeCapability)
    }

    private suspend fun constructAllAvailableDirections(pools: List<Tuple2<RelativeMultiLocation, RelativeMultiLocation>>): List<AssetConversionEdge> {
        return buildList {
            pools.forEach { (firstLocation, secondLocation) ->
                val firstAsset =
                    multiLocationConverter.chainAssetFromRelativeLocation(firstLocation, chain)
                        ?: return@forEach
                val secondAsset =
                    multiLocationConverter.chainAssetFromRelativeLocation(secondLocation, chain)
                        ?: return@forEach

                add(AssetConversionEdge(firstAsset, secondAsset))
                add(AssetConversionEdge(secondAsset, firstAsset))
            }
        }
    }

    context(withRuntime: WithRuntime, builder: ExtrinsicBuilder)
    private suspend fun executeSwap(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        sendTo: AccountId
    ) {
        builder.call(executeSwapCall(trade, swapLimit, sendTo))
    }

    context(withRuntime: WithRuntime)
    private suspend fun executeSwapCall(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        sendTo: AccountId
    ): GenericCall.Instance {
        val assetIdXcmVersion = xcmVersionDetector.detectAssetIdXcmVersion(chain.id, withRuntime.runtime)

        val path = listOf(trade.fromAsset, trade.toAsset)
            .map { asset ->
                multiLocationConverter.encodableMultiLocationOf(
                    asset,
                    assetIdXcmVersion
                )
            }

        val keepAlive = false

        return when (swapLimit) {
            is SwapLimit.SpecifiedIn -> composeCall(
                moduleName = Modules.ASSET_CONVERSION,
                callName = "swap_exact_tokens_for_tokens",
                arguments = mapOf(
                    "path" to path,
                    "amount_in" to swapLimit.amountIn.value,
                    "amount_out_min" to swapLimit.amountOutMin.value,
                    "send_to" to sendTo.value,
                    "keep_alive" to keepAlive
                )
            )

            is SwapLimit.SpecifiedOut -> composeCall(
                moduleName = Modules.ASSET_CONVERSION,
                callName = "swap_tokens_for_exact_tokens",
                arguments = mapOf(
                    "path" to path,
                    "amount_out" to swapLimit.amountOut.value,
                    "amount_in_max" to swapLimit.amountInMax.value,
                    "send_to" to sendTo.value,
                    "keep_alive" to keepAlive
                )
            )
        }
    }

    private fun List<GenericEvent.Instance>.determineSwapOutcome(): SwapOutcome {
        val swap = findEventOrThrow(Modules.ASSET_CONVERSION, "SwapExecuted")
        val (_, _, _, amountOut) = swap.arguments

        return SwapOutcome(bindBalance(amountOut))
    }

    inner class AssetConversionEdge(
        override val fromAsset: Chain.Asset,
        override val toAsset: Chain.Asset,
    ) : AssetHubSwapEdge {
        override suspend fun quote(
            amount: Balance,
            direction: SwapDirection
        ): Balance {
            return quoter.quote(fromAsset, toAsset, amount, direction)
        }
    }
}
