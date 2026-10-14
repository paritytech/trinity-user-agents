package io.paritytech.polkadotapp.common.presentation.camera

import androidx.camera.core.Camera
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.ZoomSuggestionOptions
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage

/** Decodes [formats] (ML Kit `Barcode.FORMAT_*`) and reports every code in a frame with its format. */
class QrCodeAnalyzer(
    private val formats: List<Int>,
    private val listener: (codes: List<Pair<Int, String>>) -> Unit,
) : ImageAnalysis.Analyzer, AutoCloseable {
    constructor(listener: (String) -> Unit) : this(listOf(Barcode.FORMAT_QR_CODE), { codes ->
        codes.firstOrNull()?.let { (_, text) -> listener(text) }
    })

    private var camera: Camera? = null
    private var scanner: BarcodeScanner? = null

    fun attachCamera(camera: Camera) {
        this.camera = camera
    }

    // ML Kit's BarcodeScanner owns a native detector + worker thread pool. Without close() every bind leaks one.
    override fun close() {
        scanner?.close()
    }

    override fun analyze(imageProxy: ImageProxy) {
        val mediaImage = imageProxy.image
        if (mediaImage == null) {
            imageProxy.close()
            return
        }

        val image = InputImage.fromMediaImage(mediaImage, imageProxy.imageInfo.rotationDegrees)
        val scanner = scanner ?: createScanner().also { scanner = it }

        scanner.process(image)
            .addOnSuccessListener {
                val codes = it.mapNotNull { barcode -> barcode.rawValue?.let { text -> barcode.format to text } }
                if (codes.isNotEmpty()) listener(codes)
            }
            .addOnCompleteListener {
                imageProxy.close()
            }
    }

    private fun createScanner(): BarcodeScanner {
        val optionsBuilder = BarcodeScannerOptions.Builder()
            .setBarcodeFormats(formats.first(), *formats.drop(1).toIntArray())

        zoomSuggestionOptions()?.let(optionsBuilder::setZoomSuggestionOptions)

        return BarcodeScanning.getClient(optionsBuilder.build())
    }

    private fun zoomSuggestionOptions(): ZoomSuggestionOptions? {
        val camera = camera ?: return null
        val maxZoomRatio = camera.cameraInfo.zoomState.value?.maxZoomRatio ?: return null

        return ZoomSuggestionOptions.Builder { zoomRatio ->
            camera.cameraControl.setZoomRatio(zoomRatio)
            true
        }
            .setMaxSupportedZoomRatio(maxZoomRatio)
            .build()
    }
}
