package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr

import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.isValidSubstrateAddress
import io.paritytech.polkadotapp.common.presentation.camera.CameraQrReader
import io.paritytech.polkadotapp.common.presentation.camera.QrCodeAnalyzer
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.emit
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.SendPaymentFragment
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class ScanAddressQrViewModel @Inject constructor(
    private val router: PocketRouter,
    private val cameraQrReader: CameraQrReader
) : BaseViewModel(), ScanAddressQrContract {
    override val surfaceRequest = MutableStateFlow<SurfaceRequest?>(null)
    override val invalidAddressEvent = MutableSharedFlow<Unit>()

    override suspend fun bindToCamera(lifecycleOwner: LifecycleOwner) {
        cameraQrReader.bind(
            preview = Preview.Builder().build().apply {
                setSurfaceProvider { newSurfaceRequest ->
                    surfaceRequest.value = newSurfaceRequest
                }
            },
            lifecycleOwner = lifecycleOwner,
            qrCodeAnalyzer = QrCodeAnalyzer { handleQrCodeData(it) }
        )
    }

    override fun back() {
        router.back()
    }

    private fun handleQrCodeData(data: String) = viewModelScope.launchUnit {
        if (data.isValidSubstrateAddress()) {
            router.backWithResult(
                SendPaymentFragment.REQUEST_KEY,
                ScanAddressQrResultPayload(address = data.trim())
            )
        } else {
            invalidAddressEvent.emit()
        }
    }
}
