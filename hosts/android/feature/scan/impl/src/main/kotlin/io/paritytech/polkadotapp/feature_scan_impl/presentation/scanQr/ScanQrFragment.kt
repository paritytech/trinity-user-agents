package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr.compose.ScanQrScreen

@AndroidEntryPoint
class ScanQrFragment : BaseComposeFragment<ScanQrViewModel>() {
    override val viewModel: ScanQrViewModel by viewModels()

    @Composable
    override fun Screen() = ScanQrScreen(viewModel)
}
