package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/** When a Pocket card was tapped, on the host's clock and on the wall clock the page can compare. */
class HostPocketTap(val atMillis: Long, val atEpochMillis: Long)

/**
 * The Pocket card tap the next page open answers, so the page's load can say how long it took from the
 * tap. One slot: the Pocket opens one card at a time, and closing it forgets the tap, so a page held
 * from an earlier open, which starts no new load, cannot hand its tap to a later one.
 */
@Singleton
class HostPocketTaps @Inject constructor(
    private val timeProvider: HostMonotonicTimeProvider,
    private val wallClock: HostWallClock,
    private val capture: HostEventCapture,
) {
    private class Pending(val productId: String, val tap: HostPocketTap)

    private val last = AtomicReference<Pending?>(null)

    /** Called from the tap handler itself, before any hop, so both clocks read the tap. */
    fun tapped(productId: String) {
        if (capture.enabled) {
            last.set(Pending(productId, HostPocketTap(timeProvider.nowMillis(), wallClock.nowEpochMillis())))
        }
    }

    fun closed() {
        last.set(null)
    }

    /** The tap on [productId]'s card, if the page opening or shown now is the one that tap asked for. */
    fun consume(productId: String): HostPocketTap? {
        val pending = last.get()?.takeIf { it.productId == productId } ?: return null
        return pending.tap.takeIf { last.compareAndSet(pending, null) }
    }
}
