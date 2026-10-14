package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model

import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.bindBlockNumber
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.chains.network.binding.bindPermill
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import java.math.BigInteger

class StableSwapPoolInfo(
    val poolAssetId: HydraDxAssetId,
    val assets: List<HydraDxAssetId>,
    val initialAmplification: BigInteger,
    val finalAmplification: BigInteger,
    val initialBlock: BlockNumber,
    val finalBlock: BlockNumber,
    val fee: Fraction,
)

fun bindStablePoolInfo(decoded: Any?, poolTokenId: HydraDxAssetId): StableSwapPoolInfo {
    val struct = decoded.castToStruct()

    return StableSwapPoolInfo(
        poolAssetId = poolTokenId,
        assets = bindList(decoded["assets"], ::bindNumber),
        initialAmplification = bindNumber(struct["initialAmplification"]),
        finalAmplification = bindNumber(struct["finalAmplification"]),
        initialBlock = bindBlockNumber(struct["initialBlock"]),
        finalBlock = bindBlockNumber(struct["finalBlock"]),
        fee = bindPermill(struct["fee"])
    )
}
