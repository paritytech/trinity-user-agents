package io.paritytech.polkadotapp.app.root.presentation.root

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ScanPanelRequests @Inject constructor() {
    val openRequests: SharedFlow<Unit>
        field = MutableSharedFlow(extraBufferCapacity = 1)

    fun requestOpen() {
        openRequests.tryEmit(Unit)
    }
}
