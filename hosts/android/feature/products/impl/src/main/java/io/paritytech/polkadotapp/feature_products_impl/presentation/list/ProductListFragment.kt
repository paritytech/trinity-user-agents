package io.paritytech.polkadotapp.feature_products_impl.presentation.list

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose.ProductListScreen

@AndroidEntryPoint
class ProductListFragment : BaseComposeFragment<ProductListViewModel>() {
    override val viewModel by viewModels<ProductListViewModel>()

    @Composable
    override fun Screen() {
        ProductListScreen(viewModel)
    }
}
