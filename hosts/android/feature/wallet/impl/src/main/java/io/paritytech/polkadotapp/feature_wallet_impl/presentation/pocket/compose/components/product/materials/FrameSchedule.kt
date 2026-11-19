package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

internal class FrameSchedule {
    private var running = false
    private var released = false
    private var pending = false
    private var startNanos = 0L

    fun onChange() {
        pending = true
    }

    fun pause() {
        running = false
    }

    fun resume(nowNanos: Long) = start(nowNanos)

    fun release() {
        released = true
        running = false
    }

    fun wantsFrame(animating: Boolean): Boolean = running && (pending || animating)

    fun next(animating: Boolean): Boolean {
        val draw = wantsFrame(animating)
        if (draw) pending = false
        return draw
    }

    fun seconds(nowNanos: Long): Float = if (running) (nowNanos - startNanos) / NANOS_PER_SECOND else 0f

    private fun start(nowNanos: Long) {
        if (released) return
        running = true
        pending = true
        startNanos = nowNanos
    }

    private companion object {
        const val NANOS_PER_SECOND = 1_000_000_000f
    }
}
