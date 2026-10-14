package io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose.ProductSettingsScreen

@AndroidEntryPoint
class ProductSettingsFragment : BaseComposeFragment<ProductSettingsViewModel>() {
    override val viewModel by viewModels<ProductSettingsViewModel>()

    @Composable
    override fun Screen() {
        ProductSettingsScreen(viewModel)
    }
}
