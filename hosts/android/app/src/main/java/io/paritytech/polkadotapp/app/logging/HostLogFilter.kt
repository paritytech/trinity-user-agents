package io.paritytech.polkadotapp.app.logging

/** What the gate decided about one log. */
sealed interface HostLogVerdict {
    /**
     * Send it. [dropped] counts the lines the window that just closed turned away under the same
     * key, and is zero when it turned none away.
     */
    data class Send(val dropped: Int) : HostLogVerdict

    /** Do not send it. */
    data object Drop : HostLogVerdict
}

/**
 * Decides which logs leave the device, at the last gate before the SDK sends one.
 *
 * A log goes out only when it carries a tag this host writes, so a line that reaches the logger by
 * a route nobody planned is turned away here rather than sent. What is left sends at most
 * [LINES_PER_WINDOW] lines per [WINDOW_MS] under one key, and a `truapi.core` line counts against
 * its marker rather than the whole tag, so one noisy marker cannot hide the others. The first line
 * after a window that turned any away carries [DROPPED_ATTRIBUTE], which counts them, so a tag or
 * marker stuck in a loop costs that many lines a minute and says how many it stood in for.
 */
class HostLogFilter internal constructor(private val nowMs: () -> Long) {
    constructor() : this({ System.nanoTime() / 1_000_000 })

    private class Window(var startMs: Long) {
        var sent = 0
        var dropped = 0
    }

    private val windows = HashMap<String, Window>()

    /** Whether a log carrying [tag] and [body] may be sent. */
    fun verdict(tag: String?, body: String): HostLogVerdict {
        if (tag == null || !tag.startsWith(TAG_PREFIX)) return HostLogVerdict.Drop
        return admit(key(tag, body))
    }

    /** What a log counts against: its marker where it has one, else its tag. */
    private fun key(tag: String, body: String): String {
        val marker = coreMarker(tag, body) ?: return tag
        return "$tag $marker"
    }

    /**
     * The marker a `truapi.core` body names. A core event's body is the marker on its own, and a
     * core log line writes it before its fields, so a body naming none inside [MARKER_LIMIT] names
     * none at all and counts against its tag, which is what keeps one long line from growing the
     * table.
     */
    private fun coreMarker(tag: String, body: String): String? {
        if (tag != CORE_TAG) return null
        val head = body.take(MARKER_LIMIT)
        val markerEnd = head.indexOf(MARKER_SEPARATOR)
        return when {
            markerEnd > 0 -> head.substring(0, markerEnd)
            markerEnd == 0 -> null
            head.length == MARKER_LIMIT -> null
            else -> head.ifEmpty { null }
        }
    }

    /** Counts a line against its window. */
    private fun admit(key: String): HostLogVerdict = synchronized(windows) {
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
            return HostLogVerdict.Drop
        }
        window.sent++
        HostLogVerdict.Send(closedWithDrops)
    }

    internal companion object {
        const val TAG_PREFIX = "truapi."
        const val CORE_TAG = "truapi.core"
        const val MARKER_SEPARATOR = ": "
        const val MARKER_LIMIT = 96
        const val LINES_PER_WINDOW = 60
        const val WINDOW_MS = 60_000L
        const val DROPPED_ATTRIBUTE = "dropped"
    }
}
