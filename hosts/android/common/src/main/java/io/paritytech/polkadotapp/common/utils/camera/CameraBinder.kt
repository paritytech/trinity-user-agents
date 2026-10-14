package io.paritytech.polkadotapp.common.utils.camera

import android.content.Context
import androidx.camera.core.CameraSelector
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.camera.core.UseCase
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.awaitCancellation
import javax.inject.Inject

class CameraBinder @Inject constructor(
    @param:ApplicationContext private val context: Context
) {
    suspend fun bind(
        lifecycleOwner: LifecycleOwner,
        surfaceRequest: (SurfaceRequest) -> Unit,
        externalUseCase: UseCase
    ) {
        val processCameraProvider = ProcessCameraProvider.getInstance(context).get()

        val preview = Preview.Builder()
            .build()
            .apply {
                setSurfaceProvider(surfaceRequest)
            }

        runCatching {
            processCameraProvider.unbindAll()

            processCameraProvider.bindToLifecycle(
                lifecycleOwner,
                CameraSelector.DEFAULT_BACK_CAMERA,
                preview,
                externalUseCase
            )
            awaitCancellation()
        }

        processCameraProvider.unbindAll()
    }
}
