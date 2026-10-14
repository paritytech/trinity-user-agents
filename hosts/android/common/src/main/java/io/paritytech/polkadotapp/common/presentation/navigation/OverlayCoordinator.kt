package io.paritytech.polkadotapp.common.presentation.navigation

import io.paritytech.polkadotapp.common.utils.awaitTrue
import kotlinx.coroutines.flow.MutableStateFlow
import java.io.Closeable
import javax.inject.Inject
import javax.inject.Singleton

interface OverlayCoordinator {
    interface OverlayLock : Closeable {
        fun release()

        override fun close() {
            release()
        }
    }

    suspend fun forbidOverlays(): OverlayLock

    suspend fun awaitOverlaysAllowed()
}

@Singleton
internal class RealOverlayCoordinator @Inject constructor() : OverlayCoordinator {
    private val overlaysAllowed = MutableStateFlow(true)

    override suspend fun forbidOverlays(): OverlayCoordinator.OverlayLock {
        overlaysAllowed.value = false

        return RealOverlayLock()
    }

    override suspend fun awaitOverlaysAllowed() {
        overlaysAllowed.awaitTrue()
    }

    private inner class RealOverlayLock : OverlayCoordinator.OverlayLock {
        override fun release() {
            overlaysAllowed.value = true
        }
    }
}
