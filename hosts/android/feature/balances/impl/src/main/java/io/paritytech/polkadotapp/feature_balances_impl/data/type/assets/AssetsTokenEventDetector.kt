package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets

import io.novasama.substrate_sdk_android.extensions.requireHexPrefix
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.RuntimeType
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.novasama.substrate_sdk_android.runtime.definitions.types.toHexUntyped
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.AssetsAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.palletNameOrDefault
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.chains.util.requireAssets
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.TokenEventDetector
import io.paritytech.polkadotapp.feature_balances_api.domain.model.DepositEvent
import java.math.BigInteger
import javax.inject.Inject
import javax.inject.Singleton

class AssetsTokenEventDetector(
    private val runtimeSnapshot: RuntimeSnapshot,
    private val chainAsset: Chain.Asset
) : TokenEventDetector {
    @Singleton
    class Factory @Inject constructor(
        private val chainRegistry: ChainRegistry,
    ) {
        suspend fun create(chainAsset: Chain.Asset): AssetsTokenEventDetector {
            val runtime = chainRegistry.getRuntime(chainAsset.chainId)
            return AssetsTokenEventDetector(runtime, chainAsset)
        }
    }

    private val assetType = chainAsset.requireAssets()
    private val targetAssetId = assetType.id.stringAssetId()

    override fun detectDeposit(event: GenericEvent.Instance): DepositEvent? {
        return detectTokensDeposited(event)
    }

    private fun detectTokensDeposited(event: GenericEvent.Instance): DepositEvent? {
        if (!event.instanceOf(assetType.palletNameOrDefault(), "Issued")) return null

        val (assetId, who, amount) = event.arguments

        val assetIdType = event.event.arguments.first()!!
        val assetIdAsString = decodedAssetItToString(assetId, assetIdType)
        if (assetIdAsString != targetAssetId) return null

        return DepositEvent(
            destination = bindAccountId(who),
            amount = bindBalance(amount)
        )
    }

    private fun decodedAssetItToString(assetId: Any?, assetIdType: RuntimeType<*, *>): String {
        return if (assetId is BigInteger) {
            assetId.toString()
        } else {
            assetIdType.toHexUntyped(runtimeSnapshot, assetId).requireHexPrefix()
        }
    }

    private fun AssetsAssetId.stringAssetId(): String {
        return when (this) {
            is AssetsAssetId.Number -> value.toString()
            is AssetsAssetId.ScaleEncoded -> scaleHex
        }
    }
}
