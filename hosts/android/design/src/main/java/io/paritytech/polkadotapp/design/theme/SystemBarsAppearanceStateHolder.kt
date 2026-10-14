package io.paritytech.polkadotapp.design.theme

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject
import javax.inject.Singleton

// during navigation the incoming screen acquires before the outgoing releases.
@Singleton
class SystemBarsAppearanceStateHolder @Inject constructor() {
    private var forceDarkBackgroundRequests = 0
    private val forceDarkBackgroundState = MutableStateFlow(false)

    val forceDarkBackground: StateFlow<Boolean> = forceDarkBackgroundState.asStateFlow()

    // Main-thread only (composition effects).
    fun acquireForceDarkBackground() {
        forceDarkBackgroundRequests++
        forceDarkBackgroundState.value = true
    }

    fun releaseForceDarkBackground() {
        forceDarkBackgroundRequests = (forceDarkBackgroundRequests - 1).coerceAtLeast(0)
        forceDarkBackgroundState.value = forceDarkBackgroundRequests > 0
    }
}
