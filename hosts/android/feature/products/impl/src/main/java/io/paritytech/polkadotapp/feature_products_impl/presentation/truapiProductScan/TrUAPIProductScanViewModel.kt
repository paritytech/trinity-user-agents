package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan

import android.Manifest
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.camera.CameraQrReader
import io.paritytech.polkadotapp.common.presentation.camera.QrCodeAnalyzer
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductScanFormats
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductScanRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import uniffi.truapi.HostScan
import uniffi.truapi.ScanFilter
import uniffi.truapi.ScanVerdict
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject

@HiltViewModel
class TrUAPIProductScanViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val prompt: TrUAPIPrompt<ProductScanRequest, HostScan>,
    private val permissionAsker: PermissionAsker,
    private val cameraQrReader: CameraQrReader,
) : BaseViewModel() {
    private val filter = ScanFilter(prompt.question.request)
    private val answered = AtomicBoolean(false)

    val productId = prompt.question.productId
    val hint = prompt.question.request.hint
    val surfaceRequest = MutableStateFlow<SurfaceRequest?>(null)
    val cameraPermissionDenied = MutableStateFlow(false)
    val notForThisProduct = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    init {
        prompt.markShown()
    }

    suspend fun bindToCamera(lifecycleOwner: LifecycleOwner) {
        when (permissionAsker.askPermission(Manifest.permission.CAMERA)) {
            PermissionResult.GRANTED -> Unit
            PermissionResult.DENIED -> return answer(HostScan.CameraUnavailable)
            // The dialog tells the user how to turn it back on; closing it answers.
            PermissionResult.DENIED_FOREVER -> {
                cameraPermissionDenied.value = true
                return
            }
        }
        val formats = ProductScanFormats.mlKitFormats(prompt.question.request.formats)
        try {
            cameraQrReader.bind(
                preview = Preview.Builder().build().apply { setSurfaceProvider { surfaceRequest.value = it } },
                lifecycleOwner = lifecycleOwner,
                qrCodeAnalyzer = QrCodeAnalyzer(formats, ::onCode),
            )
        } finally {
            surfaceRequest.value = null
        }
    }

    fun onCode(mlKitFormat: Int, text: String) {
        val (format, code) = ProductScanFormats.codeFor(mlKitFormat, text) ?: return
        when (filter.observe(format, code)) {
            ScanVerdict.ACCEPT -> answer(HostScan.Scanned(code, format))
            ScanVerdict.NOT_FOR_THIS_PRODUCT -> notForThisProduct.tryEmit(Unit)
            ScanVerdict.IGNORE -> Unit
        }
    }

    fun onPermissionAlertClosed() = answer(HostScan.CameraUnavailable)

    fun onCloseClicked() = answer(HostScan.Dismissed)

    override fun onCleared() {
        prompt.dismiss()
        filter.close()
        super.onCleared()
    }

    private fun answer(scan: HostScan) {
        if (!answered.compareAndSet(false, true)) return
        prompt.answer(scan)
        launchUnit { router.back() }
    }
}
