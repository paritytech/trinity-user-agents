package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.compose.ScanAddressQrScreen

@AndroidEntryPoint
class ScanAddressQrFragment : BaseComposeFragment<ScanAddressQrViewModel>() {
    override val viewModel: ScanAddressQrViewModel by viewModels()

    @Composable
    override fun Screen() = ScanAddressQrScreen(viewModel)
}
