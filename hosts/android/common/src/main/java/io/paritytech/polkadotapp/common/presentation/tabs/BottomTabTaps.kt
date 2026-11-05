package io.paritytech.polkadotapp.common.presentation.tabs

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import javax.inject.Inject
import javax.inject.Singleton

/** Taps on any Docklet tab, including the one already shown, which selects nothing new and so is only seen here. */
@Singleton
class BottomTabTaps @Inject constructor() {
    val taps: SharedFlow<Unit>
        field = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    fun tap() {
        taps.tryEmit(Unit)
    }
}
