package io.paritytech.polkadotapp.feature_balances_impl.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.TokenEventDetector
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.data.type.issuer.TokenIssuer
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.AssetsTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.AssetsTokenEventDetector
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.AssetsTokenIssuer
import io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm.HydrationEvmExternalBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm.HydrationEvmTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType.NativeExternalBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType.NativeTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType.NativeTokenEventDetector
import io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType.NativeTokenIssuer
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.OrmlExternalBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.OrmlTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.OrmlTokenEventDetector
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.OrmlTokenIssuer
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealTokenBalanceTypeRegistry @Inject constructor(
    private val nativeFactory: NativeTokenBalanceType.Factory,
    private val assetsFactory: AssetsTokenBalanceType.Factory,
    private val ormlFactory: OrmlTokenBalanceType.Factory,
    private val hydrationEvmFactory: HydrationEvmTokenBalanceType.Factory,
    private val unsupportedBalanceSource: UnsupportedTokenBalanceType,
    private val externalNativeFactory: NativeExternalBalanceType.Factory,
    private val externalOrmlFactory: OrmlExternalBalanceType.Factory,
    private val externalHydrationEvmFactory: HydrationEvmExternalBalanceType.Factory,
    private val nativeIssuerFactory: NativeTokenIssuer.Factory,
    private val ormlIssuerFactory: OrmlTokenIssuer.Factory,
    private val assetsIssuerFactory: AssetsTokenIssuer.Factory,
    private val nativeEventDetectorFactory: NativeTokenEventDetector.Factory,
    private val assetsEventDetectorFactory: AssetsTokenEventDetector.Factory,
    private val ormlEventDetectorFactory: OrmlTokenEventDetector.Factory,
) : TokenBalanceTypeRegistry {
    override fun typeFor(chainAsset: Chain.Asset): TokenBalanceType {
        return when (val type = chainAsset.type) {
            Type.Native -> nativeFactory.create(chainAsset)
            is Type.Assets -> assetsFactory.create(chainAsset)

            is Type.Orml -> when (type.subType) {
                Type.Orml.SubType.DEFAULT -> ormlFactory.create(chainAsset)

                Type.Orml.SubType.HYDRATION_EVM -> hydrationEvmFactory.create(chainAsset)
            }

            Type.Unsupported -> unsupportedBalanceSource
        }
    }

    override suspend fun externalTypeFor(
        chainId: ChainId,
        assetId: ExternalAssetId
    ): ExternalTokenBalanceType {
        return when (assetId) {
            is ExternalAssetId.HydrationEvm -> externalHydrationEvmFactory.create(chainId, assetId)
            ExternalAssetId.Native -> externalNativeFactory.create(chainId)
            is ExternalAssetId.Orml -> externalOrmlFactory.create(chainId, assetId)
        }
    }

    override suspend fun issuerFor(chainAsset: Chain.Asset): TokenIssuer {
        return when (chainAsset.type) {
            Type.Native -> nativeIssuerFactory.create(chainAsset)
            is Type.Assets -> assetsIssuerFactory.create(chainAsset)
            is Type.Orml -> ormlIssuerFactory.create(chainAsset)
            Type.Unsupported -> error("Unsupported token")
        }
    }

    override suspend fun eventDetectorFor(chainAsset: Chain.Asset): TokenEventDetector {
        return when (chainAsset.type) {
            Type.Native -> nativeEventDetectorFactory.create(chainAsset)
            is Type.Assets -> assetsEventDetectorFactory.create(chainAsset)
            is Type.Orml -> ormlEventDetectorFactory.create(chainAsset)
            Type.Unsupported -> error("Unsupported token")
        }
    }
}
