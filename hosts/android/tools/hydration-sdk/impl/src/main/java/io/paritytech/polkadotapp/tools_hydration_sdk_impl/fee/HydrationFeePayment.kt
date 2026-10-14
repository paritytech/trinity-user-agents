package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.chains.util.utilityAsset
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.findDijkstraPathsBetween
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.measureExecution
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.CustomAssetFeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.HydrationGraph
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.HydrationFeeInjector
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.HydrationFeeInjector.ResetMode
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.HydrationFeeInjector.SetFeesMode
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.HydrationFeeInjector.SetMode
import timber.log.Timber

internal class HydrationFeePayment @AssistedInject constructor(
    @Assisted asset: Chain.Asset,
    @Assisted private val graph: suspend () -> HydrationGraph,
    @Assisted private val chain: Chain,
    private val hydrationFeeInjector: HydrationFeeInjector,
    private val hydrationPriceConversionFallback: HydrationPriceConversionFallback,
) : CustomAssetFeePayment(asset) {
    @AssistedFactory
    interface Factory {
        fun create(
            asset: Chain.Asset,
            graph: suspend () -> HydrationGraph,
            chain: Chain
        ): HydrationFeePayment
    }

    companion object {
        private const val FEE_PAYMENT_PATHS_LIMIT = 4

        private const val FEE_QUOTE_BUFFER = 1.1
    }

    override suspend fun modifyExtrinsicChecked(
        chainAsset: Chain.Asset,
        extrinsicBuilder: ExtrinsicBuilder
    ) {
        val setFeesMode = SetFeesMode(SetMode.Always, ResetMode.Never)
        hydrationFeeInjector.setFees(extrinsicBuilder, chainAsset, setFeesMode)
    }

    override suspend fun convertNativeFeeChecked(
        chainAsset: Chain.Asset,
        nativeFee: Fee
    ): Fee {
        val fee = quoteFee(chainAsset, nativeFee)
            .getOrElse {
                hydrationPriceConversionFallback.convertNativeAmount(nativeFee.amount, chainAsset)
            }

        return SimpleFee(fee, chainAsset)
    }

    private suspend fun quoteFee(
        chainAsset: Chain.Asset,
        nativeFee: Fee
    ): Result<Balance> {
        return runCatching {
            val paths = findPathCandidates(chainAsset)
            val quotes = paths.mapAsync { path -> quotePathBuy(path, nativeFee.amount) }
                .filterNotNull()

            if (quotes.isEmpty()) {
                error("Failed to convert native fee to ${chainAsset.symbol}: no routes found")
            }

            quotes.min() * FEE_QUOTE_BUFFER.toBigDecimal()
        }
    }

    private suspend fun findPathCandidates(feeAsset: Chain.Asset): List<Path<HydrationSwapEdge>> {
        val nativeAsset = chain.utilityAsset

        return measureExecution("Finding ${feeAsset.symbol} -> ${nativeAsset.symbol} fee paths") {
            graph().findDijkstraPathsBetween(
                from = feeAsset.fullId,
                to = nativeAsset.fullId,
                limit = FEE_PAYMENT_PATHS_LIMIT,
                nodeVisitFilter = null
            )
        }
    }

    private suspend fun quotePathBuy(path: Path<HydrationSwapEdge>, amount: Balance): Balance? {
        return runCatching {
            path.foldRight(amount) { segment, currentAmount ->
                segment.quote(currentAmount, SwapDirection.SPECIFIED_OUT)
            }
        }
            .onFailure { Timber.w(it, "Failed to quote path") }
            .getOrNull()
    }
}
