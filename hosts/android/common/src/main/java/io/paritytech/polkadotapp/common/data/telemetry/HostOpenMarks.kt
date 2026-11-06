package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/** What asked for a product page: a Pocket card tap or a link. */
enum class HostOpenSource(val tag: String) {
    POCKET_CARD("pocket_card"),
    DEEPLINK("deeplink"),
}

class HostOpenMark(val atMillis: Long, val atEpochMillis: Long, val source: HostOpenSource, val sessionId: String)

@Singleton
class HostOpenMarks @Inject constructor(
    private val timeProvider: HostMonotonicTimeProvider,
    private val wallClock: HostWallClock,
    private val capture: HostEventCapture,
    private val sessionIds: HostSessionIds,
    private val cacheHits: HostCacheHits,
) {
    private class Pending(val productId: String, val mark: HostOpenMark)

    private val last = AtomicReference<Pending?>(null)

    /** Called from the tap handler itself, before any hop, so both clocks read the tap. */
    fun tapped(productId: String) {
        mark(HostOpenSource.POCKET_CARD)?.let { hold(Pending(productId, it)) }
    }

    fun linkHandled(): HostOpenMark? = mark(HostOpenSource.DEEPLINK)

    fun linked(productId: String, link: HostOpenMark?) {
        link?.let { hold(Pending(productId, it)) }
    }

    fun closed() {
        hold(null)
    }

    fun pendingSession(productId: String): String? = last.get()?.takeIf { it.productId == productId }?.mark?.sessionId

    fun consume(productId: String): HostOpenMark? {
        val pending = last.get()?.takeIf { it.productId == productId } ?: return null
        if (!last.compareAndSet(pending, null)) return null
        if (timeProvider.nowMillis() - pending.mark.atMillis <= PRODUCT_OPEN_TIME_MAX_AGE_MS) return pending.mark
        cacheHits.forget(pending.mark.sessionId)
        return null
    }

    private fun hold(pending: Pending?) {
        last.getAndSet(pending)?.let { cacheHits.forget(it.mark.sessionId) }
    }

    private fun mark(source: HostOpenSource): HostOpenMark? = if (capture.enabled) {
        HostOpenMark(timeProvider.nowMillis(), wallClock.nowEpochMillis(), source, sessionIds.next())
    } else {
        null
    }

    companion object {
        const val PRODUCT_OPEN_TIME_MAX_AGE_MS = 5 * 60 * 1_000L
    }
}
