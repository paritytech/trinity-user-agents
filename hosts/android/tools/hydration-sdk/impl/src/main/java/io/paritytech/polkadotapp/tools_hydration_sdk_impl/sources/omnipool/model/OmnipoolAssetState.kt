package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model

import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import java.math.BigInteger

class OmnipoolAssetState(
    val tokenId: HydraDxAssetId,
    val hubReserve: BigInteger,
    val shares: BigInteger,
    val protocolShares: BigInteger,
    val tradeability: Tradeability
)

fun bindOmnipoolAssetState(decoded: Any?, tokenId: HydraDxAssetId): OmnipoolAssetState {
    val struct = decoded.castToStruct()

    return OmnipoolAssetState(
        tokenId = tokenId,
        hubReserve = bindNumber(struct["hubReserve"]),
        shares = bindNumber(struct["shares"]),
        protocolShares = bindNumber(struct["protocolShares"]),
        tradeability = bindTradeability(struct["tradable"])
    )
}
