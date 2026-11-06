package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/** What asked for a product page: a Pocket card tap or a link. */
enum class HostOpenSource(val tag: String) {
    POCKET_CARD("pocket_card"),
    DEEPLINK("deeplink"),
}

class HostPocketTap(val atMillis: Long, val atEpochMillis: Long, val source: HostOpenSource)

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
        mark(HostOpenSource.POCKET_CARD)?.let { last.set(Pending(productId, it)) }
    }

    fun linkHandled(): HostPocketTap? = mark(HostOpenSource.DEEPLINK)

    fun linked(productId: String, link: HostPocketTap?) {
        link?.let { last.set(Pending(productId, it)) }
    }

    fun closed() {
        last.set(null)
    }

    fun consume(productId: String): HostPocketTap? {
        val pending = last.get()?.takeIf { it.productId == productId } ?: return null
        if (!last.compareAndSet(pending, null)) return null
        return pending.tap.takeIf { timeProvider.nowMillis() - it.atMillis <= PRODUCT_OPEN_TIME_MAX_AGE_MS }
    }

    private fun mark(source: HostOpenSource): HostPocketTap? =
        if (capture.enabled) HostPocketTap(timeProvider.nowMillis(), wallClock.nowEpochMillis(), source) else null

    companion object {
        const val PRODUCT_OPEN_TIME_MAX_AGE_MS = 5 * 60 * 1_000L
    }
}
