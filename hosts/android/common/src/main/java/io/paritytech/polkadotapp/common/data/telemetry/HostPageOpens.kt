package io.paritytech.polkadotapp.common.data.telemetry

import io.paritytech.polkadotapp.common.utils.logFailure
import java.util.UUID
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

@Singleton
class HostPageOpens @Inject constructor(
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
    private val clock: HostMonotonicTimeProvider,
) {
    private class Held(val productId: String, val open: HostPageOpen)

    private val held = AtomicReference<Held?>(null)

    /** A new ask, read now. */
    fun ask(source: HostOpenSource) =
        HostPageOpen(source, System.currentTimeMillis(), clock.nowMillis(), UUID.randomUUID().toString())

    fun hold(productId: String, open: HostPageOpen) {
        held.getAndSet(Held(productId, open))?.let { report(it.open, reused = false) }
    }

    fun take(productId: String): HostPageOpen? {
        val open = settle(productId) ?: return null
        report(open, reused = false)
        return open.takeIf { clock.nowMillis() - it.monotonicMs in 0..MAX_AGE_MS }
    }

    fun reuse(productId: String) {
        settle(productId)?.let { report(it, reused = true) }
    }

    fun giveUp(productId: String) {
        settle(productId)?.let { report(it, reused = false) }
    }

    private fun settle(productId: String): HostPageOpen? {
        val current = held.get()?.takeIf { it.productId == productId } ?: return null
        return current.open.takeIf { held.compareAndSet(current, null) }
    }

    private fun report(open: HostPageOpen, reused: Boolean) {
        runCatching {
            capture.capture { originProvider.current().pocketOpened(open, reused) }
        }.logFailure("Failed to capture native host Pocket open event")
    }

    companion object {
        const val MAX_AGE_MS = 5 * 60 * 1_000L
    }
}
