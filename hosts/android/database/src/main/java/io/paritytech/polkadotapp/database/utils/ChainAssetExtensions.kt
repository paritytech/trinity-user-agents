package io.paritytech.polkadotapp.database.utils

import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal

fun ChainAssetLocal.fullId(): FullChainAssetIdLocal {
    return FullChainAssetIdLocal(this.chainId, this.id)
}
