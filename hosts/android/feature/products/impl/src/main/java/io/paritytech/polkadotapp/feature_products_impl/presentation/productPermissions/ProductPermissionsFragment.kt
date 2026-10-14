package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose.ProductPermissionsScreen

@AndroidEntryPoint
class ProductPermissionsFragment : BaseComposeFragment<ProductPermissionsViewModel>() {
    override val viewModel by viewModels<ProductPermissionsViewModel>()

    @Composable
    override fun Screen() {
        ProductPermissionsScreen(viewModel)
    }
}
