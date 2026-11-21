package io.paritytech.polkadotapp.feature_products_api.presentation.widget

/**
 * How a keyed pan paces its moves to the worker, so the worker's answer is laid out in the touch's own frame.
 *
 * Two parts, both on unless switched off for measuring with a property read once when the app starts:
 * - `debug.gesture.send_on_touch` (`0` turns it off): a move goes out while its touch is handled, at most one a
 *   refresh, instead of waiting for the frame callback.
 * - `debug.gesture.answer_wait_ms` (0 through [LONGEST_WAIT_MS], default [DEFAULT_WAIT_MS]): after sending, the main
 *   thread waits this long at most for the answer, so it is laid out before this frame is drawn. A late answer is laid
 *   out in the next frame, as before.
 */
internal object GesturePacingSettings {
    const val DEFAULT_WAIT_MS = 6.0
    const val LONGEST_WAIT_MS = 10.0

    /** Moves go out at most this share of a refresh apart, so a touch and its frame never both send. */
    const val REFRESH_SHARE = 0.6

    private const val NANOS_PER_MS = 1_000_000.0
    private const val NANOS_PER_SECOND = 1_000_000_000.0
    private const val DEFAULT_REFRESH_HZ = 60f

    /** A system property's value, read for the measuring switches; null when it cannot be read. */
    fun property(name: String): String? = runCatching {
        Class.forName("android.os.SystemProperties").getMethod("get", String::class.java).invoke(null, name) as? String
    }.getOrNull()

    fun sendOnTouch(setting: String?): Boolean = when (setting?.trim()?.lowercase()) {
        "0", "false", "no", "off" -> false
        else -> true
    }

    fun answerWaitNanos(setting: String?): Long {
        val ms = setting?.trim()?.toDoubleOrNull()?.takeIf { it.isFinite() } ?: DEFAULT_WAIT_MS
        return (ms.coerceIn(0.0, LONGEST_WAIT_MS) * NANOS_PER_MS).toLong()
    }

    /** The shortest time between two moves for a display refreshing [refreshHz] times a second. */
    fun minimumGapNanos(refreshHz: Float?): Long {
        val hz = refreshHz?.takeIf { it.isFinite() && it > 1f } ?: DEFAULT_REFRESH_HZ
        return (NANOS_PER_SECOND / hz * REFRESH_SHARE).toLong()
    }

    val currentSendOnTouch: Boolean = sendOnTouch(property("debug.gesture.send_on_touch"))
    val currentAnswerWaitNanos: Long = answerWaitNanos(property("debug.gesture.answer_wait_ms"))
}
