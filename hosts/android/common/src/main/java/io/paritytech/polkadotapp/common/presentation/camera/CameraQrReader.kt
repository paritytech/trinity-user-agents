package io.paritytech.polkadotapp.common.presentation.camera

import android.content.Context
import android.util.Size
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.lifecycle.awaitInstance
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.awaitCancellation
import java.util.concurrent.Executors
import javax.inject.Inject

interface CameraQrReader {
    suspend fun bind(preview: Preview, lifecycleOwner: LifecycleOwner, qrCodeAnalyzer: QrCodeAnalyzer)
}

class RealCameraQrReader @Inject constructor(
    @param:ApplicationContext private val appContext: Context
) : CameraQrReader {
    override suspend fun bind(preview: Preview, lifecycleOwner: LifecycleOwner, qrCodeAnalyzer: QrCodeAnalyzer) {
        val processCameraProvider = ProcessCameraProvider.awaitInstance(appContext)
        val resolutionSelector = ResolutionSelector.Builder()
            .setResolutionStrategy(
                ResolutionStrategy(
                    Size(1920, 1080),
                    ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER
                )
            )
            .build()

        val imageAnalysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setResolutionSelector(resolutionSelector)
            .build()
        val camera = processCameraProvider.bindToLifecycle(
            lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, imageAnalysis, preview
        )
        qrCodeAnalyzer.attachCamera(camera)

        val analysisExecutor = Executors.newSingleThreadExecutor()
        imageAnalysis.setAnalyzer(analysisExecutor, qrCodeAnalyzer)

        try {
            awaitCancellation()
        } finally {
            imageAnalysis.clearAnalyzer()
            processCameraProvider.unbind(imageAnalysis, preview)
            analysisExecutor.execute(qrCodeAnalyzer::close)
            analysisExecutor.shutdown()
        }
    }
}
