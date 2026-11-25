package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
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

interface FundingCore {
    suspend fun start(): Result<Unit>

    suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?>

    suspend fun session(intentId: FundingIntentId): Result<FundingSession?>

    suspend fun progress(intentId: FundingIntentId): Result<FundingProgress?>

    suspend fun options(intentId: FundingIntentId): Result<FundingOptions?>

    suspend fun quotes(intentId: FundingIntentId): Result<FundingQuoteList?>

    suspend fun summary(intentId: FundingIntentId): Result<FundingSessionSummary?>

    suspend fun entries(): Result<List<FundingBookEntry>>

    suspend fun history(): Result<List<FundingSessionSummary>>

    suspend fun select(intentId: FundingIntentId, selection: FundingSelection): Result<Unit>

    suspend fun requestQuote(intentId: FundingIntentId, amount: Balance): Result<ULong?>

    suspend fun startWhenQuoted(intentId: FundingIntentId, requestId: ULong): Result<Unit>

    suspend fun amountIssue(intentId: FundingIntentId, amount: Balance, spendable: Balance?): Result<FundingAmountIssue?>

    suspend fun selectProvider(intentId: FundingIntentId, providerId: ProductId, quoteId: String): Result<Boolean>

    suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome>

    suspend fun confirmCancel(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation>

    suspend fun acknowledge(intentId: FundingIntentId): Result<Boolean>

    suspend fun openFrame(intentId: FundingIntentId): Result<Boolean>

    suspend fun frameWaiting(intentId: FundingIntentId): Result<Boolean>
}
