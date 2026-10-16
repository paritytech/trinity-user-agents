package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.selectAsset

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.SimpleAssetListFragment
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleTokenListUiConfig
import kotlinx.collections.immutable.persistentListOf

@AndroidEntryPoint
class SelectAssetFragment : SimpleAssetListFragment<SelectAssetViewModel>() {
    override val viewModel: SelectAssetViewModel by viewModels()

    @Composable
    override fun config() = SimpleTokenListUiConfig(
        titleRes = R.string.asset_details_fund_digital_dollar,
        titleArgs = persistentListOf(LocalPaymentAssetBrand.current.symbol)
    )
}
