package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.ProductId
import uniffi.truapi.U128

/**
 * The part of the TrUAPI core the funding screens drive. The core owns every session, says when one changes and
 * answers what the screens draw; the screens report what the user picked, price the session and hand it to a
 * provider.
 */
interface FundingRuntime {
    /**
     * Opens a session on the host's own behalf, which the core then shows the overlay for, for [amount] in the CASH
     * asset's planks; `null` lets the user choose. Answers the session id, or `null` when the user dismissed it.
     */
    suspend fun openFunding(direction: FundingDirection, amount: U128? = null): Result<String?>

    /** Starts the core, which then pushes every session in flight and every ended one the host has not acknowledged. */
    suspend fun start(): Result<Unit>

    /** Whether the host offers any funding provider, as the core last answered; `false` until it has. */
    val available: StateFlow<Boolean>

    /** Every session the core holds in full, with its progress, by intent, as the core last answered. */
    val sessions: StateFlow<Map<String, FundingBookEntry>>

    /**
     * Every ended session the core holds, in full or compacted, as summaries newest first, as the core last answered:
     * read again when the core starts and whenever a session changes.
     */
    val history: StateFlow<List<FundingSessionSummary>>

    /** Whether session [intent] is still open, so its overlay can be shown again; `false` once it ended. */
    suspend fun sessionOpen(intent: String): Boolean

    /** Ended session [intent]'s summary, as history lists it; `null` while it is open or for no such session. */
    suspend fun summary(intent: String): Result<FundingSessionSummary?>

    /** What session [intent]'s overlay offers, as the core last answered; `null` until it has. */
    fun options(intent: String): Flow<FundingOptions?>

    /** The user's rail, asset, network and country in session [intent]'s overlay; the options for it follow. */
    suspend fun selectOptions(intent: String, selection: FundingSelection)

    /**
     * Asks every provider routing the selected rail to price [amount] for session [intent], on what [selectOptions]
     * last reported. The list arrives through [quoteLists], and is asked for again every minute until the session is
     * handed to a provider, ends or leaves the overlay, or another amount or selection replaces it. Returns the
     * request id the list carries, `null` while the selection names no asset.
     */
    suspend fun requestQuote(intent: String, amount: U128): Result<ULong?>

    /**
     * Why [amount] cannot go ahead in session [intent], if it cannot: more than [spendable] on a withdrawal, then what
     * every provider's refusal of this amount said, then the amount limits its options show.
     */
    suspend fun amountIssue(intent: String, amount: U128, spendable: U128?): Result<FundingAmountIssue?>

    /**
     * Starts session [intent] on the best quote of request [requestId] once every provider answered it, at once when
     * they already have; the start comes through [sessions]. Another quote request or selection, [cancelAutoStart] or
     * a list with no usable quote disarms it.
     */
    suspend fun startWhenQuoted(intent: String, requestId: ULong)

    /** Disarms [startWhenQuoted] for session [intent], as the user leaves its quotes. */
    suspend fun cancelAutoStart(intent: String)

    suspend fun selectProvider(intent: String, providerId: ProductId, quoteId: String): Result<Boolean>

    /** The user's cancel, and what it did. */
    suspend fun cancel(intent: String): Result<CancelOutcome>

    /** The user's answer to the provider's cancel offer: the terms shown to accept them, `null` to decline. */
    suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation>

    /** Records that the user was shown how ended session [intent] went. */
    suspend fun acknowledge(intent: String): Result<Boolean>

    /**
     * Session [intent]'s quote list each time it changed, from when this is collected on: the current request's,
     * or `null` once a selection on another route withdrew it. An earlier one is not replayed. Never completes: the
     * collector stops it.
     */
    fun quoteLists(intent: String): Flow<FundingQuoteList?>

    /**
     * Emits the intent of every session whose status changed, once [sessions] holds the change; look the session up
     * there. A change before collection is not replayed, and it never completes.
     */
    fun sessionChanges(): Flow<String>
}

/** Session [intent] as the core last answered. */
fun FundingRuntime.session(intent: String): FundingSession? = sessions.value[intent]?.session

/** Session [intent]'s progress, as the core last answered. */
fun FundingRuntime.progress(intent: String): FundingProgress? = sessions.value[intent]?.progress
