package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountry
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import javax.inject.Inject

class TrUAPIFundingRuntime @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val bridge: AppFundingHostBridge,
) : FundingRuntime {
    override suspend fun openFunding(direction: FundingDirection): Result<String?> =
        runtimeProvider.runtime().mapCatching { it.openFunding(direction, null) }

    override suspend fun session(intent: String): FundingSession? = read { it.fundingSession(intent) }

    override suspend fun progress(intent: String): FundingProgress? = read { it.fundingProgress(intent) }

    override suspend fun sessions(): List<FundingSession> = read { it.fundingSessions() }.orEmpty()

    override suspend fun candidates(intent: String): List<FundingCandidate> = read { it.fundingCandidates(intent) }.orEmpty()

    override suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<Unit> =
        runtimeProvider.runtime().mapCatching { it.getFundingQuote(intent, ask) }

    override suspend fun stopQuotes(intent: String) {
        read { it.stopFundingQuotes(intent) }
    }

    override suspend fun countries(intent: String, rail: FundingRail, asset: String): List<FundingCountry> =
        read { it.fundingCountries(intent, rail, asset) }.orEmpty()

    override suspend fun detectedCountry(): String? =
        runtimeProvider.runtime()
            .mapCatching { it.detectedFundingCountry() }
            .logFailure("TrUAPI funding country read failed")
            .getOrNull()

    override suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.selectFundingProvider(intent, providerId, quoteId) }

    override suspend fun cancel(intent: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.cancelFunding(intent) }

    override suspend fun acknowledge(intent: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.acknowledgeFundingSession(intent) }

    override fun quoteLists(intent: String): Flow<FundingQuoteList> =
        bridge.quoteLists().filter { it.intent == intent }.map { it.quotes }

    override fun sessionChanges(): Flow<String> = bridge.changedSessions()

    private suspend fun <T> read(block: (TrUAPIHostRuntime) -> T): T? =
        runtimeProvider.runtime()
            .mapCatching(block)
            .logFailure("TrUAPI funding read failed")
            .getOrNull()
}
