package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr

import android.Manifest
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.camera.CameraQrReader
import io.paritytech.polkadotapp.common.presentation.camera.QrCodeAnalyzer
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.emit
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@ViewModelScoped
class QrScanningMixin @Inject constructor(
    private val permissionAsker: PermissionAsker,
    private val cameraQrReader: CameraQrReader,
    private val parsers: Set<@JvmSuppressWildcards ScanContentParser>,
) {
    val surfaceRequest = MutableStateFlow<SurfaceRequest?>(null)

    val invalidCodeEvent = MutableSharedFlow<Unit>()

    val cameraPermissionDenied = MutableStateFlow(false)

    val cameraPermissionMissing = MutableStateFlow(false)

    val postParseActions = MutableSharedFlow<PostParseAction>()

    private var pauseDecoding = false

    private var recognitionArmed = true

    context(scope: ComputationalScope)
    suspend fun bindToCamera(lifecycleOwner: LifecycleOwner) {
        resetScanning()

        when (permissionAsker.askPermission(Manifest.permission.CAMERA)) {
            PermissionResult.GRANTED -> Unit

            PermissionResult.DENIED -> {
                cameraPermissionMissing.enable()
                return
            }

            PermissionResult.DENIED_FOREVER -> {
                cameraPermissionMissing.enable()
                cameraPermissionDenied.enable()
                return
            }
        }

        try {
            cameraQrReader.bind(
                preview = Preview.Builder().build().apply {
                    setSurfaceProvider { newSurfaceRequest ->
                        surfaceRequest.value = newSurfaceRequest
                    }
                },
                lifecycleOwner = lifecycleOwner,
                qrCodeAnalyzer = QrCodeAnalyzer { handleQrCodeData(it) }
            )
        } finally {
            surfaceRequest.value = null
        }
    }

    fun invalidationDialogClosed() {
        pauseDecoding = false
    }

    fun permissionAlertClosed() {
        cameraPermissionDenied.disable()
    }

    fun setRecognitionArmed(armed: Boolean) {
        recognitionArmed = armed
    }

    // The panel host keeps this alive across panel open/close, so a stale decode gate would leave the scanner
    // dead on re-entry and a stale SurfaceRequest would point at an already released surface.
    private fun resetScanning() {
        pauseDecoding = false
        surfaceRequest.value = null
        cameraPermissionDenied.disable()
        cameraPermissionMissing.disable()
    }

    context(scope: ComputationalScope)
    private fun handleQrCodeData(data: String) {
        if (pauseDecoding || !recognitionArmed) return

        pauseDecoding = true

        scope.launchUnit {
            val parser = parsers.firstOrNull { it.canHandle(data) }
            if (parser == null) {
                invalidCodeEvent.emit()
                return@launchUnit
            }

            parser.handle(data)
                .onSuccess { postParseActions.emit(it) }
                .onFailure { invalidCodeEvent.emit() }
        }
    }
}
