package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.ScanAddressQrResultPayload
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.SendPaymentScreen

@AndroidEntryPoint
class SendPaymentFragment : BaseComposeFragment<SendPaymentViewModel>() {
    override val viewModel: SendPaymentViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        observeResult<ScanAddressQrResultPayload>(REQUEST_KEY) { payload ->
            viewModel.onQrResult(payload)
        }
    }

    @Composable
    override fun Screen() = SendPaymentScreen(viewModel)

    companion object {
        const val REQUEST_KEY = "scan_qr_address"
    }
}
