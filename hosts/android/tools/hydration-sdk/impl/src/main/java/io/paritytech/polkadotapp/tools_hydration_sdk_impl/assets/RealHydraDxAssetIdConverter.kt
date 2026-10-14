package io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyIdOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.bindNumberOrNull
import java.math.BigInteger
import javax.inject.Inject

private val SYSTEM_ON_CHAIN_ASSET_ID = BigInteger.ZERO

internal class RealHydraDxAssetIdConverter @Inject constructor(
    private val chainRegistry: ChainRegistry
) : HydraDxAssetIdConverter {
    override val systemAssetId: HydraDxAssetId = SYSTEM_ON_CHAIN_ASSET_ID

    override suspend fun toOnChainIdOrNull(chainAsset: Chain.Asset): HydraDxAssetId? {
        val runtime = chainRegistry.getRuntime(chainAsset.chainId)
        return chainAsset.hydrationAssetIdOrNull(runtime)
    }

    override suspend fun toChainAssetOrNull(chain: Chain, onChainId: HydraDxAssetId): Chain.Asset? {
        val runtime = chainRegistry.getRuntime(chain.id)

        return chain.assets.find { chainAsset ->
            val omniPoolId = chainAsset.hydrationAssetIdOrNull(runtime)

            omniPoolId == onChainId
        }
    }

    override suspend fun allOnChainIds(chain: Chain): Map<HydraDxAssetId, Chain.Asset> {
        val runtime = chainRegistry.getRuntime(chain.id)

        return chain.assets.mapNotNull { chainAsset ->
            chainAsset.hydrationAssetIdOrNull(runtime)?.let { it to chainAsset }
        }.toMap()
    }

    private fun Chain.Asset.hydrationAssetIdOrNull(runtimeSnapshot: RuntimeSnapshot): HydraDxAssetId? {
        return when (val type = type) {
            is Chain.Asset.Type.Orml -> bindNumberOrNull(type.currencyIdOrNull(runtimeSnapshot)?.value)
            is Chain.Asset.Type.Native -> systemAssetId
            else -> null
        }
    }
}
