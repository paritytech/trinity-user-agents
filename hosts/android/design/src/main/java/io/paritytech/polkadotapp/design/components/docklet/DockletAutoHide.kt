package io.paritytech.polkadotapp.design.components.docklet

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class DockletAutoHide(private val scope: CoroutineScope, private val onHide: () -> Unit) {
    private var outsideJob: Job? = null
    private var idleJob: Job? = null

    var blocked: Boolean = true
        set(value) {
            if (field == value) return
            field = value
            if (value) cancelAll() else restartIdle()
        }

    fun onTouch(touch: DockletTouch) {
        if (blocked) return
        if (touch == DockletTouch.Docklet) {
            outsideJob?.cancel()
            outsideJob = null
        } else if (outsideJob?.isActive != true) {
            outsideJob = scope.launch {
                delay(OutsideDelay)
                onHide()
            }
        }
        restartIdle()
    }

    fun onKey() {
        if (blocked) return
        outsideJob?.cancel()
        outsideJob = null
        restartIdle()
    }

    fun onWheel() = onTouch(DockletTouch.Outside)

    private fun restartIdle() {
        idleJob?.cancel()
        idleJob = scope.launch {
            delay(IdleDelay)
            onHide()
        }
    }

    private fun cancelAll() {
        outsideJob?.cancel()
        idleJob?.cancel()
        outsideJob = null
        idleJob = null
    }

    private companion object {
        val OutsideDelay: Duration = Duration.ZERO
        val IdleDelay: Duration = 3.seconds
    }
}
