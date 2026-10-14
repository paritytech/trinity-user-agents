package io.paritytech.polkadotapp.feature_balances_api.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.TokenEventDetector
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.data.type.issuer.TokenIssuer

interface TokenBalanceTypeRegistry {
    fun typeFor(chainAsset: Chain.Asset): TokenBalanceType

    suspend fun externalTypeFor(chainId: ChainId, assetId: ExternalAssetId): ExternalTokenBalanceType

    suspend fun issuerFor(chainAsset: Chain.Asset): TokenIssuer

    suspend fun eventDetectorFor(chainAsset: Chain.Asset): TokenEventDetector
}
