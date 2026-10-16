package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction

import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Guards the startup clear of uncommitted handoff marks, which drops every mark the device holds.
 *
 * Two conditions make that clear safe, and both are properties of the process rather than of a screen. It
 * belongs to a genuine process start, so a recreated Activity must not repeat it; and no reservation may be
 * live while it runs — a send that has marked its coins but not yet committed would lose the only record
 * keeping those coins from being selected again, while their keys are already on their way to a peer.
 */
@Singleton
class CoinageHandoffGuard @Inject constructor() {
    private val liveHandoffs = AtomicInteger(0)
    private val startupReleaseClaimed = AtomicBoolean(false)

    fun handoffReserved() {
        liveHandoffs.incrementAndGet()
    }

    fun handoffSettled() {
        liveHandoffs.decrementAndGet()
    }

    /** True at most once per process, and never while a reservation is live — a live one skips, never waits. */
    fun claimStartupRelease(): Boolean {
        val live = liveHandoffs.get()
        if (live > 0) {
            coinageLogW("handoff-startup-release-skipped live=$live")
            return false
        }

        return startupReleaseClaimed.compareAndSet(false, true)
    }
}
