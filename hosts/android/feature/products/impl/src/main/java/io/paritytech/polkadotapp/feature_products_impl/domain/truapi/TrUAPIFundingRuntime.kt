package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSessionSummary
import javax.inject.Inject

class TrUAPIFundingRuntime @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val bridge: AppFundingHostBridge,
) : FundingRuntime {
    override suspend fun openFunding(direction: FundingDirection): Result<String?> =
        runtimeProvider.runtime().mapCatching { it.openFunding(direction, null) }

    override suspend fun start(): Result<Unit> = runtimeProvider.runtime().map { }

    override val sessions: StateFlow<Map<String, FundingSessionEntry>> = bridge.sessions()

    override val inProgress: StateFlow<Boolean?> = bridge.inProgress()

    override suspend fun history(): Result<List<FundingSessionSummary>> =
        runtimeProvider.runtime().mapCatching { it.fundingHistory() }

    override fun options(intent: String): Flow<FundingOptions?> =
        bridge.options().map { it[intent] }.distinctUntilChanged()

    override suspend fun selectOptions(intent: String, selection: FundingSelection) {
        runtimeProvider.runtime()
            .mapCatching { it.selectFundingOptions(intent, selection) }
            .logFailure("TrUAPI funding selection failed")
    }

    override suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<ULong> =
        runtimeProvider.runtime().mapCatching { it.getFundingQuote(intent, ask) }

    override suspend fun stopQuotes(intent: String) {
        runtimeProvider.runtime()
            .mapCatching { it.stopFundingQuotes(intent) }
            .logFailure("TrUAPI funding quotes could not stop")
    }

    override suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.selectFundingProvider(intent, providerId, quoteId) }

    override suspend fun cancel(intent: String): Result<CancelOutcome> =
        runtimeProvider.runtime().mapCatching { it.cancelFunding(intent) }

    override suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation> =
        runtimeProvider.runtime().mapCatching { it.confirmFundingCancel(intent, terms) }

    override suspend fun acknowledge(intent: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.acknowledgeFundingSession(intent) }

    override fun quoteLists(intent: String): Flow<FundingQuoteList> =
        bridge.quoteLists().filter { it.intent == intent }.map { it.quotes }

    override fun sessionChanges(): Flow<String> = bridge.changedSessions()
}
