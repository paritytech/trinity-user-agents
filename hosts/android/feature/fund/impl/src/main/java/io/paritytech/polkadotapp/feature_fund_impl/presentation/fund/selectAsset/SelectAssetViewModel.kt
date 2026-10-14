package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.selectAsset

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.feature_fund_api.domain.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_fund_impl.FundRouter
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.selectAsset.SelectFundAssetInteractor
import io.paritytech.polkadotapp.feature_tokens_api.domain.AssetDisplayMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.toAssetPayload
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.SimpleAssetListViewModel
import javax.inject.Inject

@HiltViewModel
class SelectAssetViewModel @Inject constructor(
    private val router: FundRouter,
    private val interactor: SelectFundAssetInteractor,
    autoConvertDepositService: AutoConvertDepositService,
    assetDisplayMapper: AssetDisplayMapper,
) : SimpleAssetListViewModel(assetDisplayMapper) {
    init {
        autoConvertDepositService.initiateDepositTermsWarmUp()
    }

    override fun backClicked() = router.back()

    override suspend fun assets() = interactor.depositAssets()

    override fun tokenClicked(asset: Chain.Asset) {
        router.openFund(asset.fullId.toAssetPayload())
    }
}
