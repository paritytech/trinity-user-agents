package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import java.math.BigInteger

interface HydrationFeeInjector {
    class SetFeesMode(
        val setMode: SetMode,
        val resetMode: ResetMode
    )

    sealed class SetMode {
        /**
         * Always sets the fee to the required token, regardless of whether fees are already in the needed state or not
         */
        object Always : SetMode()

        /**
         * Sets the fee token to the required one only the current fee payment asset is different
         */
        class Lazy(val currentlySetFeeAsset: BigInteger) : SetMode()
    }

    sealed class ResetMode {
        /**
         * Always resets the fee to the native token, regardless of whether fees are already in the needed state or not
         */
        object ToNative : ResetMode()

        /**
         * Resets the fee to the native one only the current fee payment asset is different
         */
        class ToNativeLazily(val feeAssetBeforeTransaction: BigInteger) : ResetMode()

        /**
         * Never reset the fee payment asset - leave the one which was set
         */
        object Never : ResetMode()
    }

    suspend fun setFees(
        extrinsicBuilder: ExtrinsicBuilder,
        paymentAsset: Chain.Asset,
        mode: SetFeesMode,
    )
}
