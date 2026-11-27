package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
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
import javax.inject.Inject

class TrUAPIFundingCore @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : FundingCore {
    override suspend fun start(): Result<Unit> = runtimeProvider.runtime().map { }

    override suspend fun available(): Result<Boolean> = runtimeProvider.runtime().mapCatching { it.fundingAvailable() }

    override suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?> =
        runtimeProvider.runtime().flatMap { runtime -> runCancellableCatching { runtime.openFunding(direction, amount?.toU128())?.let(::FundingIntentId) } }

    override suspend fun session(intentId: FundingIntentId): Result<FundingSession?> =
        runtimeProvider.runtime().mapCatching { it.fundingSession(intentId.value) }

    override suspend fun progress(intentId: FundingIntentId): Result<FundingProgress?> =
        runtimeProvider.runtime().mapCatching { it.fundingProgress(intentId.value) }

    override suspend fun options(intentId: FundingIntentId): Result<FundingOptions?> =
        runtimeProvider.runtime().mapCatching { it.fundingOptions(intentId.value) }

    override suspend fun quotes(intentId: FundingIntentId): Result<FundingQuoteList?> =
        runtimeProvider.runtime().mapCatching { it.fundingQuotes(intentId.value) }

    override suspend fun summary(intentId: FundingIntentId): Result<FundingSessionSummary?> =
        runtimeProvider.runtime().mapCatching { it.fundingSummary(intentId.value) }

    override suspend fun entries(): Result<List<FundingBookEntry>> =
        runtimeProvider.runtime().mapCatching { it.fundingEntries() }

    override suspend fun history(): Result<List<FundingSessionSummary>> =
        runtimeProvider.runtime().mapCatching { it.fundingHistory() }

    override suspend fun select(intentId: FundingIntentId, selection: FundingSelection): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.selectFundingOptions(intentId.value, selection) }

    override suspend fun requestQuote(intentId: FundingIntentId, amount: Balance): Result<ULong?> =
        runtimeProvider.runtime().mapCatching { it.getFundingQuote(intentId.value, amount.toU128()) }

    override suspend fun startWhenQuoted(intentId: FundingIntentId, requestId: ULong): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.startFundingWhenQuoted(intentId.value, requestId) }

    override suspend fun amountIssue(intentId: FundingIntentId, amount: Balance, spendable: Balance?): Result<FundingAmountIssue?> =
        runtimeProvider.runtime().mapCatching { it.fundingAmountIssue(intentId.value, amount.toU128(), spendable?.toU128()) }

    override suspend fun selectProvider(intentId: FundingIntentId, providerId: ProductId, quoteId: String): Result<Boolean> =
        runtimeProvider.runtime().flatMap { runtime -> runCancellableCatching { runtime.selectFundingProvider(intentId.value, providerId, quoteId) } }

    override suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome> =
        runtimeProvider.runtime().flatMap { runtime -> runCancellableCatching { runtime.cancelFunding(intentId.value) } }

    override suspend fun confirmCancel(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation> =
        runtimeProvider.runtime().flatMap { runtime -> runCancellableCatching { runtime.confirmFundingCancel(intentId.value, terms) } }

    override suspend fun acknowledge(intentId: FundingIntentId): Result<Boolean> =
        runtimeProvider.runtime().flatMap { runtime -> runCancellableCatching { runtime.acknowledgeFundingSession(intentId.value) } }

    override suspend fun openFrame(intentId: FundingIntentId): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.openProviderFrame(intentId.value) }

    override suspend fun frameWaiting(intentId: FundingIntentId): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.fundingFrameWaiting(intentId.value) }
}
