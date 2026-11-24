package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import timber.log.Timber
import javax.inject.Inject

/**
 * Forwards `truapi.*` lines at info and above to the [HostLogSink], at most [LINES_PER_WINDOW] per tag
 * in each [WINDOW_MS] window. A `truapi.core` line counts against its marker rather than the whole tag,
 * so one noisy marker cannot hide the others. The first line after a window that dropped lines is
 * preceded by one line that counts them, so a product or connection stuck in a loop sends at most that
 * many lines a minute plus the count. A count that no later line follows, because the tag went quiet or
 * the app stopped, is never sent.
 */
class HostLogSinkTree internal constructor(
    private val sink: HostLogSink,
    private val nowMs: () -> Long,
) : Timber.Tree() {
    @Inject
    constructor(sink: HostLogSink) : this(sink, { System.nanoTime() / 1_000_000 })

    private class Window(var startMs: Long) {
        var sent = 0
        var dropped = 0
    }

    private val windows = HashMap<String, Window>()

    override fun isLoggable(tag: String?, priority: Int): Boolean =
        priority >= Log.INFO && tag?.startsWith(TAG_PREFIX) == true

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        val name = tag.orEmpty()
        val marker = coreMarker(name, message)
        val key = if (marker == null) name else "$name $marker"
        val dropped = synchronized(windows) {
            val now = nowMs()
            val window = windows.getOrPut(key) { Window(now) }
            var closedWithDrops = 0
            if (now - window.startMs >= WINDOW_MS) {
                closedWithDrops = window.dropped
                window.startMs = now
                window.sent = 0
                window.dropped = 0
            }
            if (window.sent >= LINES_PER_WINDOW) {
                window.dropped++
                return
            }
            window.sent++
            closedWithDrops
        }
        if (dropped > 0) {
            sink.log(
                Log.WARN,
                name,
                "$dropped $key lines dropped over the limit of $LINES_PER_WINDOW a minute",
                buildMap {
                    put(DROPPED_ATTRIBUTE, dropped)
                    if (marker != null) put(MARKER_ATTRIBUTE, marker)
                },
            )
        }
        sink.log(priority, name, message, emptyMap())
    }

    /** The marker a `truapi.core` line counts against, or null for every other line. */
    private fun coreMarker(tag: String, message: String): String? {
        if (tag != CORE_TAG) return null
        val markerEnd = message.indexOf(MARKER_SEPARATOR)
        return if (markerEnd > 0) message.substring(0, markerEnd) else null
    }

    internal companion object {
        const val TAG_PREFIX = "truapi."
        const val CORE_TAG = "truapi.core"
        const val MARKER_SEPARATOR = ": "
        const val LINES_PER_WINDOW = 60
        const val WINDOW_MS = 60_000L
        const val DROPPED_ATTRIBUTE = "dropped"
        const val MARKER_ATTRIBUTE = "marker"
    }
}
