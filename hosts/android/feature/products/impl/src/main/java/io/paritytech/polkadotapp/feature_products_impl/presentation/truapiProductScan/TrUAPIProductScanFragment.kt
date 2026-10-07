package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan.compose.TrUAPIProductScanScreen

@AndroidEntryPoint
class TrUAPIProductScanFragment : BaseComposeFragment<TrUAPIProductScanViewModel>() {
    override val viewModel: TrUAPIProductScanViewModel by viewModels()

    @Composable
    override fun Screen() = TrUAPIProductScanScreen(viewModel)
}
