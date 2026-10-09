package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.flow.Flow
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingSession

/** The part of the TrUAPI core the funding screens drive. The core owns every session; the screens read it, price it and hand it to a provider. */
interface FundingRuntime {
    suspend fun session(intent: String): FundingSession?

    suspend fun progress(intent: String): FundingProgress?

    suspend fun sessions(): List<FundingSession>

    suspend fun candidates(intent: String): List<FundingCandidate>

    /** Each provider's answer arrives through [quoteRows]. */
    suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<Unit>

    suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean>

    suspend fun cancel(intent: String): Result<Boolean>

    suspend fun acknowledge(intent: String): Result<Boolean>

    fun quoteRows(intent: String): Flow<FundingQuoteRow>

    /** Emits the intent of every session whose status changed. */
    fun sessionChanges(): Flow<String>
}
