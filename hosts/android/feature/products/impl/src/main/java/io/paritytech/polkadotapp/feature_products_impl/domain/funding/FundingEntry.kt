package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary

enum class FundingPhase {
    SELECTING,
    RUNNING,
    ENDED,
}

data class FundingEntry(
    val id: FundingIntentId,
    val direction: FundingDirection,
    val amount: Balance?,
    val openedAtMs: ULong,
    val session: FundingSession?,
    val progress: FundingProgress?,
    val summary: FundingSessionSummary?,
    val options: FundingOptions?,
    val askedAmount: Balance?,
    val quotes: FundingQuoteList?,
    val quotesFailed: Boolean,
    val isPresented: Boolean,
    val phase: FundingPhase,
) {
    val hasStarted: Boolean
        get() = phase != FundingPhase.SELECTING

    val hasEnded: Boolean
        get() = phase == FundingPhase.ENDED

    val isGone: Boolean
        get() = phase == FundingPhase.SELECTING && !isPresented

    val failure: FundingFailure?
        get() = session?.stage?.failure

    companion object {
        fun of(
            id: FundingIntentId,
            fallbackDirection: FundingDirection,
            fallbackAmount: Balance?,
            session: FundingSession?,
            progress: FundingProgress?,
            summary: FundingSessionSummary?,
            options: FundingOptions?,
            askedAmount: Balance?,
            quotes: FundingQuoteList?,
            quotesFailed: Boolean,
            isPresented: Boolean,
        ) = FundingEntry(
            id = id,
            direction = session?.direction ?: summary?.direction ?: fallbackDirection,
            amount = (session?.amount ?: summary?.amount)?.toBalanceOrNull() ?: fallbackAmount,
            openedAtMs = session?.openedAtMs ?: summary?.openedAtMs ?: 0uL,
            session = session,
            progress = progress,
            summary = summary,
            options = options,
            askedAmount = askedAmount,
            quotes = quotes,
            quotesFailed = quotesFailed,
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
