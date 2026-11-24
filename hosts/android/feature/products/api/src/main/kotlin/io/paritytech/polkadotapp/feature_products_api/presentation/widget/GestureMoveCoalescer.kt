package io.paritytech.polkadotapp.feature_products_api.presentation.widget

/**
 * Holds a keyed pan's newest move until it may go out: one move in flight until the worker answers, and at most one a
 * frame. A move goes out from the touch ([takeTouch]) or, if the touch had to hold it, from the next frame
 * ([takeFrame]). Times are `System.nanoTime()`, the clock Android's frame times use.
 */
internal class GestureMoveCoalescer<T : Any> {
    private var pending: T? = null
    private var awaitingReply = false
    private var lastFrameNanos: Long? = null
    private var lastSentNanos: Long? = null
    private var lastAnswer: Long? = null

    fun offer(move: T) {
        pending = move
    }

    /** At a touch: the newest move, unless an answer is owed or the last move went out less than [minimumGapNanos] ago. */
    fun takeTouch(nowNanos: Long, minimumGapNanos: Long): T? {
        if (awaitingReply) return null
        lastSentNanos?.let { if (nowNanos - it < minimumGapNanos) return null }
        val move = pending ?: return null
        return sent(move, nowNanos)
    }

    /** In a frame: the move a touch held, unless an answer is owed or a move already went out in this frame. */
    fun takeFrame(frameNanos: Long): T? {
        if (awaitingReply || lastFrameNanos == frameNanos) return null
        // A touch handled in this frame comes after the frame's own time, and it already sent.
        lastSentNanos?.let { if (it >= frameNanos) return null }
        val move = pending ?: return null
        lastFrameNanos = frameNanos
        return sent(move, frameNanos)
    }

    private fun sent(move: T, atNanos: Long): T {
        pending = null
        awaitingReply = true
        lastSentNanos = atNanos
        return move
    }

    /** The worker's answer count as last seen: a new count answers the move in flight. */
    fun noteAnswer(answer: Long) {
        if (lastAnswer != null && answer != lastAnswer) acknowledge()
        lastAnswer = answer
    }

    fun acknowledge() {
        awaitingReply = false
    }

    fun clear() {
        pending = null
        awaitingReply = false
    }
}
