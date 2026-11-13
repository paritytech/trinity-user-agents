package io.paritytech.polkadotapp.common.data.telemetry

import io.paritytech.polkadotapp.common.utils.logFailure
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/** What asked for a product page: a Pocket card tap or a link. */
enum class HostOpenSource(val tag: String) {
    POCKET_CARD("pocket_card"),
    DEEPLINK("deeplink"),
}

/**
 * One ask to open a product page, read where the user asked, before any hop. [monotonicMs] times the load,
 * since a wall clock can jump; [epochMs] is only what the page is told. [sessionId] is minted with it: the
 * host session that takes this ask adopts it.
 */
data class HostPageOpen(
    val source: HostOpenSource,
    val epochMs: Long,
    val monotonicMs: Long,
    val sessionId: String,
)

/**
 * The host session of one product page: [id] is what the page reads; [open] is the ask its load is timed from.
 * [isRetry] marks a session that picked an ask back up after the session that first took it ended before its
 * page was shown.
 */
data class HostPageSession(
    val productId: String,
    val id: String,
    val open: HostPageOpen?,
    val isRetry: Boolean,
)

@Singleton
class HostPageOpens @Inject constructor(
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
    private val clock: HostMonotonicTimeProvider,
) {
    private class Held(val productId: String, val open: HostPageOpen)

    private val held = AtomicReference<Held?>(null)

    // The last session started per product, so an ask that shows its page again is reported under its id.
    private val live = ConcurrentHashMap<String, HostPageSession>()

    // Asks whose session ended before its page was shown, for the product's next session to pick up.
    private val unfinished = ConcurrentHashMap<String, HostPageOpen>()
    private val shown = ConcurrentHashMap.newKeySet<HostPageSession>()

    /** A new ask, read now. */
    fun ask(source: HostOpenSource) =
        HostPageOpen(source, System.currentTimeMillis(), clock.nowMillis(), UUID.randomUUID().toString())

    fun hold(productId: String, open: HostPageOpen) {
        held.getAndSet(Held(productId, open))?.let { report(it.open, it.open.sessionId, reused = false) }
    }

    /**
     * Starts the host session of a page of [productId]: it takes the ask held for the product and adopts its id.
     * With none held, it picks back up an ask whose session ended before its page was shown, as a retry of that
     * load, with the same id and tap; else it has no ask and mints its own id. An ask too old to time the load
     * from still gives the session its id, so its open event can be joined to the page.
     */
    fun startSession(productId: String): HostPageSession {
        val open = settle(productId)
        val session = if (open != null) {
            unfinished.remove(productId)
            report(open, open.sessionId, reused = false)
            HostPageSession(productId, open.sessionId, open.takeIf(::isRecent), isRetry = false)
        } else {
            unfinished.remove(productId)?.takeIf(::isRecent)
                ?.let { HostPageSession(productId, it.sessionId, it, isRetry = true) }
                ?: HostPageSession(productId, UUID.randomUUID().toString(), open = null, isRetry = false)
        }
        live[productId] = session
        return session
    }

    /** The session's page is shown, so its ask is done: no later session of the product retries it. */
    fun pageShown(session: HostPageSession) {
        shown += session
    }

    /**
     * The session is gone, so a later ask for its product no longer shows its page. An ask it took and never
     * showed a page for is left for the product's next session to retry.
     */
    fun sessionEnded(session: HostPageSession) {
        live.remove(session.productId, session)
        if (!shown.remove(session) && session.open != null) unfinished[session.productId] = session.open
    }

    /**
     * The product's page is already running, so the held ask shows it again and starts no session. It is reported
     * under that page's session id, which is the one the page and its own events carry.
     */
    fun reuse(productId: String) {
        val open = settle(productId) ?: return
        report(open, live[productId]?.id ?: open.sessionId, reused = true)
    }

    fun giveUp(productId: String) {
        settle(productId)?.let { report(it, it.sessionId, reused = false) }
    }

    private fun isRecent(open: HostPageOpen) = clock.nowMillis() - open.monotonicMs in 0..MAX_AGE_MS

    private fun settle(productId: String): HostPageOpen? {
        val current = held.get()?.takeIf { it.productId == productId } ?: return null
        return current.open.takeIf { held.compareAndSet(current, null) }
    }

    private fun report(open: HostPageOpen, sessionId: String, reused: Boolean) {
        runCatching {
            capture.capture { originProvider.current().pocketOpened(open, sessionId, reused) }
        }.logFailure("Failed to capture native host Pocket open event")
    }

    companion object {
        const val MAX_AGE_MS = 5 * 60 * 1_000L
    }
}
