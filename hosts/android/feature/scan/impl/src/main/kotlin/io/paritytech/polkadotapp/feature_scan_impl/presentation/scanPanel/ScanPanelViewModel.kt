package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanPanel

import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr.QrScanningMixin
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

@HiltViewModel
class ScanPanelViewModel @Inject constructor(
    private val qrScanning: QrScanningMixin,
) : BaseViewModel() {
    val surfaceRequest: StateFlow<SurfaceRequest?> = qrScanning.surfaceRequest

    val invalidCodeEvent: SharedFlow<Unit> = qrScanning.invalidCodeEvent

    val cameraPermissionDenied: StateFlow<Boolean> = qrScanning.cameraPermissionDenied

    val cameraPermissionMissing: StateFlow<Boolean> = qrScanning.cameraPermissionMissing

    val scanHandled: SharedFlow<(() -> Unit)?>
        field = MutableSharedFlow(extraBufferCapacity = 1)

    init {
        qrScanning.postParseActions
            .onEach(::handlePostParseAction)
            .launchIn(this)
    }

    suspend fun bindToCamera(lifecycleOwner: LifecycleOwner) {
        qrScanning.bindToCamera(lifecycleOwner)
    }

    fun invalidationDialogClosed() {
        qrScanning.invalidationDialogClosed()
    }

    fun permissionAlertClosed() {
        qrScanning.permissionAlertClosed()
    }

    fun setRecognitionArmed(armed: Boolean) {
        qrScanning.setRecognitionArmed(armed)
    }

    private suspend fun handlePostParseAction(action: PostParseAction) {
        val navigate: (() -> Unit)? = when (action) {
            is PostParseAction.BackAndThen -> action.postBackNavigation
            PostParseAction.Nothing -> null
        }

        scanHandled.emit(navigate)
    }
}
