package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.toOnChainIdOrThrow
import javax.inject.Inject

internal class RealHydrationFeeInjector @Inject constructor(
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
) : HydrationFeeInjector {
    override suspend fun setFees(
        extrinsicBuilder: ExtrinsicBuilder,
        paymentAsset: Chain.Asset,
        mode: HydrationFeeInjector.SetFeesMode
    ) {
        val baseCalls = extrinsicBuilder.getCalls()
        extrinsicBuilder.resetCalls()

        val justSetFees = getSetPhase(mode.setMode).setFees(extrinsicBuilder, paymentAsset)
        extrinsicBuilder.addCalls(baseCalls)
        getResetPhase(mode.resetMode).resetFees(extrinsicBuilder, justSetFees)
    }

    private fun getSetPhase(mode: HydrationFeeInjector.SetMode): SetPhase {
        return when (mode) {
            HydrationFeeInjector.SetMode.Always -> AlwaysSetPhase()
            is HydrationFeeInjector.SetMode.Lazy -> LazySetPhase(mode.currentlySetFeeAsset)
        }
    }

    private fun getResetPhase(mode: HydrationFeeInjector.ResetMode): ResetPhase {
        return when (mode) {
            HydrationFeeInjector.ResetMode.ToNative -> AlwaysResetPhase()
            is HydrationFeeInjector.ResetMode.ToNativeLazily -> LazyResetPhase(mode.feeAssetBeforeTransaction)
            HydrationFeeInjector.ResetMode.Never -> NeverReset()
        }
    }

    private interface SetPhase {
        /**
         * @return just set on-chain asset id, if changed
         */
        suspend fun setFees(extrinsicBuilder: ExtrinsicBuilder, paymentAsset: Chain.Asset): HydraDxAssetId?
    }

    private interface ResetPhase {
        suspend fun resetFees(
            extrinsicBuilder: ExtrinsicBuilder,
            feesModifiedInSetPhase: HydraDxAssetId?
        )
    }

    private inner class AlwaysSetPhase : SetPhase {
        override suspend fun setFees(extrinsicBuilder: ExtrinsicBuilder, paymentAsset: Chain.Asset): HydraDxAssetId {
            val onChainId = hydraDxAssetIdConverter.toOnChainIdOrThrow(paymentAsset)
            extrinsicBuilder.setFeeCurrency(onChainId)
            return onChainId
        }
    }

    private inner class LazySetPhase(
        private val currentFeeTokenId: HydraDxAssetId,
    ) : SetPhase {
        override suspend fun setFees(extrinsicBuilder: ExtrinsicBuilder, paymentAsset: Chain.Asset): HydraDxAssetId? {
            val paymentCurrencyToSet = getPaymentCurrencyToSetIfNeeded(paymentAsset)

            paymentCurrencyToSet?.let {
                extrinsicBuilder.setFeeCurrency(paymentCurrencyToSet)
            }

            return paymentCurrencyToSet
        }

        private suspend fun getPaymentCurrencyToSetIfNeeded(expectedPaymentAsset: Chain.Asset): HydraDxAssetId? {
            val expectedPaymentTokenId = hydraDxAssetIdConverter.toOnChainIdOrThrow(expectedPaymentAsset)

            return expectedPaymentTokenId.takeIf { currentFeeTokenId != expectedPaymentTokenId }
        }
    }

    private inner class NeverReset : ResetPhase {
        override suspend fun resetFees(
            extrinsicBuilder: ExtrinsicBuilder,
            feesModifiedInSetPhase: HydraDxAssetId?
        ) {
            // no op
        }
    }

    private inner class AlwaysResetPhase : ResetPhase {
        override suspend fun resetFees(
            extrinsicBuilder: ExtrinsicBuilder,
            feesModifiedInSetPhase: HydraDxAssetId?
        ) {
            extrinsicBuilder.setFeeCurrency(hydraDxAssetIdConverter.systemAssetId)
        }
    }

    private inner class LazyResetPhase(
        private val previousFeeCurrency: HydraDxAssetId
    ) : ResetPhase {
        override suspend fun resetFees(extrinsicBuilder: ExtrinsicBuilder, feesModifiedInSetPhase: HydraDxAssetId?) {
            val justSetFeeToNonNative = feesModifiedInSetPhase != null && feesModifiedInSetPhase != hydraDxAssetIdConverter.systemAssetId
            val previousCurrencyRemainsNonNative = feesModifiedInSetPhase == null && previousFeeCurrency != hydraDxAssetIdConverter.systemAssetId

            if (justSetFeeToNonNative || previousCurrencyRemainsNonNative) {
                extrinsicBuilder.setFeeCurrency(hydraDxAssetIdConverter.systemAssetId)
            }
        }
    }

    private fun ExtrinsicBuilder.setFeeCurrency(onChainId: HydraDxAssetId) {
        call(
            moduleName = Modules.MULTI_TRANSACTION_PAYMENT,
            callName = "set_currency",
            arguments = mapOf(
                "currency" to onChainId
            )
        )
    }
}
