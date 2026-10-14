package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr

import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_impl.ScanRouter
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

@HiltViewModel
class ScanQrViewModel @Inject constructor(
    private val router: ScanRouter,
    private val qrScanning: QrScanningMixin,
) : BaseViewModel() {
    val surfaceRequest: StateFlow<SurfaceRequest?> = qrScanning.surfaceRequest

    val invalidCodeEvent: SharedFlow<Unit> = qrScanning.invalidCodeEvent

    val cameraPermissionDenied: StateFlow<Boolean> = qrScanning.cameraPermissionDenied

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

    fun back() {
        router.back()
    }

    private fun handlePostParseAction(action: PostParseAction) {
        when (action) {
            is PostParseAction.BackAndThen -> {
                router.back()
                action.postBackNavigation()
            }

            PostParseAction.Nothing -> Unit
        }
    }
}
