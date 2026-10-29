package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.FundingSessionEntry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary

/**
 * The part of the TrUAPI core the funding screens drive. The core owns every session and pushes what the screens
 * draw; the screens report what the user picked, price the session and hand it to a provider.
 */
interface FundingRuntime {
    /** Opens a session on the host's own behalf, which the core then shows the overlay for. */
    suspend fun openFunding(direction: FundingDirection): Result<String?>

    /** Starts the core, which then pushes every session in flight and every ended one the host has not recorded. */
    suspend fun start(): Result<Unit>

    /** Every session the core pushed and still holds, with its latest progress, by intent. */
    val sessions: StateFlow<Map<String, FundingSessionEntry>>

    /** Every session the host recorded, as summaries, to rebuild lost history from; `null` until the core pushed it. */
    val history: StateFlow<List<FundingSessionSummary>?>

    /** What session [intent]'s overlay offers, as the core last pushed it; `null` until it has. */
    fun options(intent: String): Flow<FundingOptions?>

    /** The user's rail, asset, network and country in session [intent]'s overlay; the options for it follow. */
    suspend fun selectOptions(intent: String, selection: FundingSelection)

    /**
     * The list arrives through [quoteLists], and is asked for again every minute until [stopQuotes]. Returns the
     * request id every list for this ask carries.
     */
    suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<ULong>

    suspend fun stopQuotes(intent: String)

    suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean>

    /** The user's cancel, and what it did. */
    suspend fun cancel(intent: String): Result<CancelOutcome>

    /** The user's answer to the provider's cancel offer: the terms shown to accept them, `null` to decline. */
    suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation>

    suspend fun acknowledge(intent: String): Result<Boolean>

    fun quoteLists(intent: String): Flow<FundingQuoteList>

    /** Emits the intent of every session whose status changed, once [sessions] holds the change. */
    fun sessionChanges(): Flow<String>
}

/** Session [intent] as the core last pushed it. */
fun FundingRuntime.session(intent: String): FundingSession? = sessions.value[intent]?.session

/** The progress the core last pushed with session [intent]. */
fun FundingRuntime.progress(intent: String): FundingProgress? = sessions.value[intent]?.progress
