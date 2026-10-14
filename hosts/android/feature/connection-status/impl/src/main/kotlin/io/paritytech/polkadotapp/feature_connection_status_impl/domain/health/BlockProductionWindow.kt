package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import kotlin.math.floor
import kotlin.time.Duration
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class BlockProductionWindow(private val window: Duration) {
    private data class Sample(val height: Int, val at: Instant)

    private val samples = ArrayDeque<Sample>()

    fun record(height: Int, at: Instant) {
        samples.addLast(Sample(height, at))
        prune(at)
    }

    fun blocksProduced(now: Instant): Int? {
        prune(now)
        val oldest = samples.firstOrNull() ?: return null
        if (oldest.at > now - window) return null

        // A reorg can put the head below the anchor; nothing measurable was produced.
        return (samples.last().height - oldest.height).coerceAtLeast(0)
    }

    fun seed(anchor: BlockProductionAnchor, at: Instant) {
        val produced = if (anchor.chainClockSpan <= window) {
            anchor.blocks
        } else {
            floor(anchor.blocks * (window / anchor.chainClockSpan)).toInt()
        }

        samples.clear()
        record(height = (anchor.headHeight - produced).coerceAtLeast(0), at = at - window)
        record(height = anchor.headHeight, at = at)
    }

    // The sample at or before the window start is the height measured from; dropping it lets a stall read as production.
    private fun prune(now: Instant) {
        val cutoff = now - window
        while (samples.size > 1 && samples[1].at <= cutoff) samples.removeFirst()
    }
}
