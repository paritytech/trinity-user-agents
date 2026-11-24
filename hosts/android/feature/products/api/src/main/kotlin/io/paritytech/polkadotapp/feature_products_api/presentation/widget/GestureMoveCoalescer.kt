package io.paritytech.polkadotapp.feature_products_api.presentation.widget

internal class GestureMoveCoalescer<T : Any> {
    private var pending: T? = null
    private var awaitingReply = false
    private var lastFrameNanos: Long? = null

    fun offer(move: T) {
        pending = move
    }

    fun takeFrame(frameNanos: Long): T? {
        if (awaitingReply || lastFrameNanos == frameNanos) return null
        val move = pending ?: return null
        pending = null
        awaitingReply = true
        lastFrameNanos = frameNanos
        return move
    }

    fun acknowledge() {
        awaitingReply = false
    }

    fun clear() {
        pending = null
        awaitingReply = false
    }
}
