package io.paritytech.polkadotapp.app.root.presentation.root

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

sealed interface DockletInput {
    data class Touch(val x: Float, val y: Float) : DockletInput

    data object Key : DockletInput

    data object Wheel : DockletInput
}

private const val BUFFERED_INPUTS = 16

class DockletInputEvents {
    val events: SharedFlow<DockletInput>
        field = MutableSharedFlow(extraBufferCapacity = BUFFERED_INPUTS)

    fun report(input: DockletInput) {
        events.tryEmit(input)
    }
}
