package io.paritytech.polkadotapp.feature_transfers_impl.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersType
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersTypeRegistry
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.nativeType.NativeTransfersType
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.orml.OrmlTokenTransfersType
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.statemine.StatemineTransfersType
import javax.inject.Inject

class RealTokenTransfersTypeRegistry @Inject constructor(
    private val native: NativeTransfersType.Factory,
    private val assets: StatemineTransfersType.Factory,
    private val orml: OrmlTokenTransfersType.Factory,
) : TokenTransfersTypeRegistry {
    override suspend fun typeFor(chainAsset: Chain.Asset): TokenTransfersType {
        return when (chainAsset.type) {
            Type.Native -> native.create(chainAsset)
            is Type.Assets -> assets.create(chainAsset)
            is Type.Orml -> orml.create(chainAsset)
            Type.Unsupported -> error("Unsupported asset for transfers: ${chainAsset.symbol}")
        }
    }
}
