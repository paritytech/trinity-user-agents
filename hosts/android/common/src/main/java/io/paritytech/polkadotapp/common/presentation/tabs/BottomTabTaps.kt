package io.paritytech.polkadotapp.common.presentation.tabs

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import javax.inject.Inject
import javax.inject.Singleton

/** Tabs tapped in the Docklet. A tap on the tab already shown selects nothing new, so it is only seen here. */
@Singleton
class BottomTabTaps @Inject constructor() {
    val taps: SharedFlow<BottomTab>
        field = MutableSharedFlow<BottomTab>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    fun tap(tab: BottomTab) {
        taps.tryEmit(tab)
    }
}
