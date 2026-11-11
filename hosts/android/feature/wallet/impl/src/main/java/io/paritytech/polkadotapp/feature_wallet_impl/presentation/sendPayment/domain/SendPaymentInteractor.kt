package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

interface SendPaymentInteractor {
    suspend fun asset(): Chain.Asset

    fun chainId(): ChainId
}

class RealSendPaymentInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : SendPaymentInteractor {
    override suspend fun asset() = chainAssetProvider.asset()

    override fun chainId() = chainAssetProvider.chainId()
}
