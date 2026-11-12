package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCore
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
import javax.inject.Inject

class TrUAPIFundingCore @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : FundingCore {
    override suspend fun start(): Result<Unit> = runtimeProvider.runtime().map { }

    override suspend fun open(direction: FundingDirection, amount: U128?): Result<String?> =
        runtimeProvider.runtime().mapCatching { it.openFunding(direction, amount) }

    override suspend fun session(intent: String): Result<FundingSession?> =
        runtimeProvider.runtime().mapCatching { it.fundingSession(intent) }

    override suspend fun progress(intent: String): Result<FundingProgress?> =
        runtimeProvider.runtime().mapCatching { it.fundingProgress(intent) }

    override suspend fun options(intent: String): Result<FundingOptions?> =
        runtimeProvider.runtime().mapCatching { it.fundingOptions(intent) }

    override suspend fun quotes(intent: String): Result<FundingQuoteList?> =
        runtimeProvider.runtime().mapCatching { it.fundingQuotes(intent) }

    override suspend fun summary(intent: String): Result<FundingSessionSummary?> =
        runtimeProvider.runtime().mapCatching { it.fundingSummary(intent) }

    override suspend fun entries(): Result<List<FundingBookEntry>> =
        runtimeProvider.runtime().mapCatching { it.fundingEntries() }

    override suspend fun history(): Result<List<FundingSessionSummary>> =
        runtimeProvider.runtime().mapCatching { it.fundingHistory() }

    override suspend fun select(intent: String, selection: FundingSelection): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.selectFundingOptions(intent, selection) }

    override suspend fun requestQuote(intent: String, amount: U128): Result<ULong?> =
        runtimeProvider.runtime().mapCatching { it.getFundingQuote(intent, amount) }

    override suspend fun startWhenQuoted(intent: String, requestId: ULong): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.startFundingWhenQuoted(intent, requestId) }

    override suspend fun cancelAutoStart(intent: String): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.cancelFundingAutoStart(intent) }

    override suspend fun amountIssue(intent: String, amount: U128, spendable: U128?): Result<FundingAmountIssue?> =
        runtimeProvider.runtime().mapCatching { it.fundingAmountIssue(intent, amount, spendable) }

    override suspend fun selectProvider(intent: String, providerId: ProductId, quoteId: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.selectFundingProvider(intent, providerId, quoteId) }

    override suspend fun cancel(intent: String): Result<CancelOutcome> =
        runtimeProvider.runtime().mapCatching { it.cancelFunding(intent) }

    override suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation> =
        runtimeProvider.runtime().mapCatching { it.confirmFundingCancel(intent, terms) }

    override suspend fun acknowledge(intent: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.acknowledgeFundingSession(intent) }
}
