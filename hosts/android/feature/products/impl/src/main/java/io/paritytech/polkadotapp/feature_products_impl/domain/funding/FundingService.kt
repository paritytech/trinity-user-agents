package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.flow.Flow
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import uniffi.truapi.U128

/**
 * Everything the funding screens ask of the core, in one place. The core owns every session: the screens follow a
 * session's state, report what the user picked, and leave every decision to the core.
 */
interface FundingService {
    /** Every session the core holds, in flight and ended, newest first: the current list, then a new one on every change. */
    fun entries(): Flow<List<FundingEntry>>

    /**
     * Session [id] as it changes, current state first, with the options and quotes for it. Completes after the
     * session ended, and without a value once it is gone: dropped by the core or left before it started.
     */
    fun entryState(id: String): Flow<FundingEntry>

    /** Starts the core, which resumes the sessions it holds. */
    suspend fun resume(): Result<Unit>

    /**
     * Opens a session on the host's own behalf, for [amount] in the balance asset's planks or what the user types when
     * `null`. Its screens are asked for through [events]; answers its id, or `null` once the user left it.
     */
    suspend fun open(direction: FundingDirection, amount: U128?): Result<String?>

    /** Tells the core what the user picked, in order with the asks after it. What it leads to comes back as options. */
    fun select(selection: FundingSelection, id: String)

    /** Asks every provider for the selection to quote [amount], or drops the quotes on screen when `null`. */
    fun quote(amount: U128?, id: String)

    /** Why [amount] cannot go ahead for the reported selection, given what a withdrawal can spend. */
    suspend fun amountIssue(amount: U128, spendable: U128?, id: String): FundingAmountIssue?

    /**
     * Hands the session to [provider] on the quote on screen. With no provider the core hands it to the latest ask's
     * best quote once every provider answered, and the session's state says when it did. Fails with a
     * [FundingStartError] when the provider did not take it.
     */
    suspend fun start(id: String, provider: ProductId?): Result<Unit>

    /** The user left the screen that started the session on its best quote: the core no longer starts it. */
    fun stopStarting(id: String)

    /** The user left the session's screens. One that never started is dropped; one in flight runs on. */
    fun leave(id: String)

    /**
     * Cancels a session. One that never started is dropped at once, as when the user leaves it; one in flight goes to
     * the core, which ends it or asks the provider.
     */
    suspend fun cancel(id: String): Result<CancelOutcome>

    /** Answers the provider's cancel offer: [terms] are the ones the user accepted, as shown, or `null` to keep going. */
    suspend fun answerCancelOffer(id: String, terms: CancelTerms?): Result<CancelConfirmation>

    /** The user was shown how session [id] ended. */
    suspend fun acknowledge(id: String)

    /** What the core asks of the app's screens. One consumer reads it; events wait until it does. */
    val events: Flow<FundingEvent>

    /** Lets the provider show the screen [request] asked for, once its session is on screen or the user tapped the notice. */
    fun openProviderFrame(request: FundingFrameRequest)

    /** Answers the provider's screen [request], once it is gone. */
    fun answerFrame(request: FundingFrameRequest, outcome: FundingFrameOutcome)
}

sealed interface FundingEvent {
    /** Show the funding screens for this new session. */
    data class PresentationRequested(val entry: FundingEntry) : FundingEvent

    /** A provider wants to show one of its screens for a session, answered through [FundingService.answerFrame]. */
    data class FrameRequested(val request: FundingFrameRequest) : FundingEvent

    /** The provider's screens for a session are done: take them down and show the session in their place. */
    data class FrameClosed(val entryId: String) : FundingEvent

    /** A provider's worker keeps crashing while the core restarts it. */
    data class ProviderNotWorking(val productId: ProductId) : FundingEvent
}

/** A provider's ask to show [target] for session [entryId]. */
data class FundingFrameRequest(
    val id: String,
    val entryId: String,
    val providerId: ProductId,
    val target: FundingFrameTarget,
)

/** Why the session could not be handed to the chosen provider. */
sealed class FundingStartError : Exception() {
    /** No quote from that provider is on screen. */
    class NoQuote : FundingStartError()

    /** The user left before the provider took the session. */
    class Left : FundingStartError()

    /** The provider would not take it; [failure] says why when the core recorded one. */
    class NotSelected(val failure: FundingFailure?) : FundingStartError()
}
