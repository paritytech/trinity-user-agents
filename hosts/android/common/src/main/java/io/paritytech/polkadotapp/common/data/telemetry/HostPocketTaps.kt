package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The Pocket card tap the next page open answers, so the page's load can say how long it took from the
 * tap. One slot: the Pocket opens one card at a time, and closing it forgets the tap, so a page held
 * from an earlier open, which starts no new load, cannot hand its tap to a later one.
 */
@Singleton
class HostPocketTaps @Inject constructor(
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    private class Tap(val productId: String, val atMillis: Long)

    private val last = AtomicReference<Tap?>(null)

    fun tapped(productId: String) {
        if (capture.enabled) last.set(Tap(productId, timeProvider.nowMillis()))
    }

    fun closed() {
        last.set(null)
    }

    /** When [productId]'s card was tapped, if the page opening now is the one that tap asked for. */
    fun consume(productId: String): Long? {
        val tap = last.get()?.takeIf { it.productId == productId } ?: return null
        return tap.atMillis.takeIf { last.compareAndSet(tap, null) }
    }
}
