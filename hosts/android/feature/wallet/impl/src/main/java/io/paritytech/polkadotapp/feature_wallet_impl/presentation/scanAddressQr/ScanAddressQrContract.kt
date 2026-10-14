package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr

import androidx.camera.core.SurfaceRequest
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

interface ScanAddressQrContract {
    val surfaceRequest: StateFlow<SurfaceRequest?>

    val invalidAddressEvent: SharedFlow<Unit>

    fun back()
    suspend fun bindToCamera(lifecycleOwner: LifecycleOwner)
}
