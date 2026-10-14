package io.paritytech.polkadotapp.tools_assethub_sdk_impl.data

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.asRawScaleValue
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.encodableMultiLocationOf
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.exception.AssetHubSwapQuoteException
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapDirection

class AssetConversionQuoter @AssistedInject constructor(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
    private val xcmVersionDetector: XcmVersionDetector,
    @Assisted private val multiLocationConverter: ChainAssetLocationConverter,
    @Assisted private val chain: Chain,
) {
    @AssistedFactory
    interface Factory {
        fun create(
            chain: Chain,
            multiLocationConverter: ChainAssetLocationConverter
        ): AssetConversionQuoter
    }

    suspend fun quote(
        fromAsset: Chain.Asset,
        toAsset: Chain.Asset,
        amount: Balance,
        direction: SwapDirection
    ): Balance {
        val runtimeCallsApi = multiChainRuntimeCallsApi.forChain(chain.id)

        return runtimeCallsApi.quote(
            swapDirection = direction,
            assetIn = fromAsset,
            assetOut = toAsset,
            amount = amount
        ) ?: throw AssetHubSwapQuoteException()
    }

    private suspend fun RuntimeCallsApi.quote(
        swapDirection: SwapDirection,
        assetIn: Chain.Asset,
        assetOut: Chain.Asset,
        amount: Balance,
    ): Balance? {
        val method = when (swapDirection) {
            SwapDirection.SPECIFIED_IN -> "quote_price_exact_tokens_for_tokens"
            SwapDirection.SPECIFIED_OUT -> "quote_price_tokens_for_exact_tokens"
        }

        val assetIdXcmVersion = xcmVersionDetector.detectAssetIdXcmVersion(chain.id, runtime)

        val asset1 = multiLocationConverter.encodableMultiLocationOf(assetIn, assetIdXcmVersion)
        val asset2 = multiLocationConverter.encodableMultiLocationOf(assetOut, assetIdXcmVersion)

        return call(
            section = "AssetConversionApi",
            method = method,
            arguments = autoEncodedArgs(
                "asset1" to asset1.asRawScaleValue(),
                "asset2" to asset2.asRawScaleValue(),
                "amount" to amount,
                "include_fee" to true
            ),
        )
    }
}
