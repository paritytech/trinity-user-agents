package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.compose.CurrencyScreen

@AndroidEntryPoint
class CurrencyFragment : BaseComposeFragment<CurrencyViewModel>() {
    override val viewModel: CurrencyViewModel by viewModels()

    @Composable
    override fun Screen() {
        CurrencyScreen(contract = viewModel)
    }
}
