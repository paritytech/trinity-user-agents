package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.compose.ProductBotManagementScreen

@AndroidEntryPoint
class ProductBotManagementFragment : BaseComposeFragment<ProductBotManagementViewModel>() {
    override val viewModel: ProductBotManagementViewModel by viewModels()

    @Composable
    override fun Screen() {
        ProductBotManagementScreen(viewModel)
    }
}
