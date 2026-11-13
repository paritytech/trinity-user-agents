package io.paritytech.polkadotapp.feature_products_impl.domain.funding

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

/** The core's funding calls, as [FundingService] makes them. Each boots the core first when it is not running. */
interface FundingCore {
    suspend fun start(): Result<Unit>

    suspend fun open(direction: FundingDirection, amount: U128?): Result<String?>

    suspend fun session(intent: String): Result<FundingSession?>

    suspend fun progress(intent: String): Result<FundingProgress?>

    suspend fun options(intent: String): Result<FundingOptions?>

    suspend fun quotes(intent: String): Result<FundingQuoteList?>

    suspend fun summary(intent: String): Result<FundingSessionSummary?>

    /** Every session the core holds in full, with its progress. */
    suspend fun entries(): Result<List<FundingBookEntry>>

    /** Every ended session, as summaries. */
    suspend fun history(): Result<List<FundingSessionSummary>>

    suspend fun select(intent: String, selection: FundingSelection): Result<Unit>

    /** The request id the quote list for [amount] carries; `null` while the selection names no asset. */
    suspend fun requestQuote(intent: String, amount: U128): Result<ULong?>

    suspend fun startWhenQuoted(intent: String, requestId: ULong): Result<Unit>

    suspend fun cancelAutoStart(intent: String): Result<Unit>

    suspend fun amountIssue(intent: String, amount: U128, spendable: U128?): Result<FundingAmountIssue?>

    suspend fun selectProvider(intent: String, providerId: ProductId, quoteId: String): Result<Boolean>

    suspend fun cancel(intent: String): Result<CancelOutcome>

    suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation>

    suspend fun acknowledge(intent: String): Result<Boolean>

    /** Lets the screen session [intent]'s provider asked for be presented; answers whether one was waiting. */
    suspend fun openFrame(intent: String): Result<Boolean>

    /** Whether session [intent]'s provider waits for its screen to be opened. */
    suspend fun frameWaiting(intent: String): Result<Boolean>
}
