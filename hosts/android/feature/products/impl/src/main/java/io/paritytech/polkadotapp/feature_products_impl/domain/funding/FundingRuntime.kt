package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.flow.Flow
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingCountry as CoreFundingCountry

/** The part of the TrUAPI core the funding screens drive. The core owns every session; the screens read it, price it and hand it to a provider. */
interface FundingRuntime {
    /** Opens a session on the host's own behalf, which the core then shows the overlay for. */
    suspend fun openFunding(direction: FundingDirection): Result<String?>

    suspend fun session(intent: String): FundingSession?

    suspend fun progress(intent: String): FundingProgress?

    suspend fun sessions(): List<FundingSession>

    suspend fun candidates(intent: String): List<FundingCandidate>

    /** The list arrives through [quoteLists], and is asked for again every minute until [stopQuotes]. */
    suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<Unit>

    suspend fun stopQuotes(intent: String)

    /** What the core knows about each country on [rail] and [asset], for the country picker. */
    suspend fun countries(intent: String, rail: FundingRail, asset: String): List<CoreFundingCountry>

    /** The country quotes go by until the user picks one. */
    suspend fun detectedCountry(): String?

    suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean>

    suspend fun cancel(intent: String): Result<Boolean>

    suspend fun acknowledge(intent: String): Result<Boolean>

    fun quoteLists(intent: String): Flow<FundingQuoteList>

    /** Emits the intent of every session whose status changed. */
    fun sessionChanges(): Flow<String>
}
