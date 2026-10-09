package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

interface SendPaymentInteractor {
    suspend fun asset(): Chain.Asset

    fun chainId(): ChainId

    /** Opens the funding overlay for value leaving the pocket: to a bank, a card or a crypto wallet. */
    suspend fun openWithdrawal(): Result<Unit>
}

class RealSendPaymentInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val hostFunding: HostFunding,
) : SendPaymentInteractor {
    override suspend fun asset() = chainAssetProvider.asset()

    override fun chainId() = chainAssetProvider.chainId()

    override suspend fun openWithdrawal(): Result<Unit> = hostFunding.openFunding(FundingDirection.OUT, amount = null).map { }
}
