package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

interface SendPaymentInteractor {
    suspend fun asset(): Chain.Asset

    fun chainId(): ChainId

    suspend fun getFundingConfig(): Result<FundingConfig>
}

class RealSendPaymentInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val fundingDomainProvider: FundingDomainProvider,
) : SendPaymentInteractor {
    override suspend fun asset() = chainAssetProvider.asset()

    override fun chainId() = chainAssetProvider.chainId()

    override suspend fun getFundingConfig() = fundingDomainProvider.getFundingConfig()
}
