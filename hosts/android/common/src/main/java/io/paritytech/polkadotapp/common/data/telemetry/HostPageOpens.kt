package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/** What asked for a product page: a Pocket card tap or a link. */
enum class HostOpenSource(val tag: String) {
    POCKET_CARD("pocket_card"),
    DEEPLINK("deeplink"),
}

/** One ask to open a product page. [epochMs] is read where the user asked, before any hop. */
data class HostPageOpen(val source: HostOpenSource, val epochMs: Long = System.currentTimeMillis())

/** Holds the latest ask until the page it opens takes it, so the page can time itself from the tap. */
@Singleton
class HostPageOpens @Inject constructor() {
    private class Held(val productId: String, val open: HostPageOpen)

    private val held = AtomicReference<Held?>(null)

    fun hold(productId: String, open: HostPageOpen) {
        held.set(Held(productId, open))
    }

    /** Drops the ask held for [productId], when the page it was for reuses a session and no new one will take it. */
    fun discard(productId: String) {
        held.get()?.takeIf { it.productId == productId }?.let { held.compareAndSet(it, null) }
    }

    /** Taken once, and only for the product it was held for; an ask older than [MAX_AGE_MS] is dropped. */
    fun take(productId: String, nowEpochMs: Long = System.currentTimeMillis()): HostPageOpen? {
        val current = held.get()?.takeIf { it.productId == productId } ?: return null
        if (!held.compareAndSet(current, null)) return null
        return current.open.takeIf { nowEpochMs - it.epochMs in 0..MAX_AGE_MS }
    }

    companion object {
        const val MAX_AGE_MS = 5 * 60 * 1_000L
    }
}
