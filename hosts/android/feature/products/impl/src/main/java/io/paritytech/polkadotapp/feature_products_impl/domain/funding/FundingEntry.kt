package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.ProductId
import uniffi.truapi.U128

enum class FundingPhase {
    /** Not handed to a provider yet. */
    SELECTING,

    /** Handed to a provider and in flight. */
    RUNNING,

    ENDED,
}

/** One funding session as the screens draw it: what the core holds for it, and the quotes of the latest ask on it. */
data class FundingEntry(
    /** The session's intent. */
    val id: String,
    val direction: FundingDirection,
    val amount: U128?,
    val openedAtMs: ULong,
    val session: FundingSession?,
    val progress: FundingProgress?,
    /** How an ended session went. */
    val summary: FundingSessionSummary?,
    /** What the core offers for the reported selection, while the session is being set up. */
    val options: FundingOptions?,
    /** The core's list for the latest ask; `null` from a new ask until its first list. */
    val quotes: FundingQuoteList?,
    /** The quotes on screen before the same amount was asked again, kept while their providers answer again. */
    val staleQuotes: Map<ProductId, FundingQuote>,
    /** An ask waits for its first list. */
    val isQuoting: Boolean,
    /** The core waits on the session's screens for the user to start it or leave. */
    val isPresented: Boolean,
    val phase: FundingPhase,
) {
    val hasStarted: Boolean
        get() = phase != FundingPhase.SELECTING

    val hasEnded: Boolean
        get() = phase == FundingPhase.ENDED

    /** A session that never started and that nothing waits on any more: the core dropped it or the user left it. */
    val isGone: Boolean
        get() = phase == FundingPhase.SELECTING && !isPresented

    /** Why the session ended, once it has failed. */
    val failure: FundingFailure?
        get() = session?.stage?.failure

    companion object {
        fun of(
            id: String,
            fallbackDirection: FundingDirection,
            fallbackAmount: U128?,
            session: FundingSession?,
            progress: FundingProgress?,
            summary: FundingSessionSummary?,
            options: FundingOptions?,
            quotes: FundingQuoteList?,
            staleQuotes: Map<ProductId, FundingQuote>,
            isQuoting: Boolean,
            isPresented: Boolean,
        ) = FundingEntry(
            id = id,
            direction = session?.direction ?: summary?.direction ?: fallbackDirection,
            amount = session?.amount ?: summary?.amount ?: fallbackAmount,
            openedAtMs = session?.openedAtMs ?: summary?.openedAtMs ?: 0uL,
            session = session,
            progress = progress,
            summary = summary,
            options = options,
            quotes = quotes,
            staleQuotes = staleQuotes,
            isQuoting = isQuoting,
            isPresented = isPresented,
            phase = phaseOf(session, summary),
        )

        private fun phaseOf(session: FundingSession?, summary: FundingSessionSummary?): FundingPhase = when {
            session == null -> if (summary == null) FundingPhase.SELECTING else FundingPhase.ENDED
            !session.stage.isOpen -> FundingPhase.ENDED
            session.providerId == null -> FundingPhase.SELECTING
            else -> FundingPhase.RUNNING
        }
    }
}
