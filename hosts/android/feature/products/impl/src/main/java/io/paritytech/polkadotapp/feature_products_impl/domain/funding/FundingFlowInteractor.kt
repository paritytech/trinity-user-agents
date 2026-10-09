package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStep
import java.math.BigDecimal
import javax.inject.Inject

class FundingFlowInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val balance: FundingBalance,
    private val runtime: FundingRuntime,
    private val branding: FundingProviderBranding,
) {
    suspend fun cash(): FundingCash {
        val asset = chainAssetProvider.asset()
        return FundingCash(symbol = asset.symbol, precision = asset.precision)
    }

    suspend fun spendable(cash: FundingCash): BigDecimal? =
        balance.spendable()?.let { cash.decimal(it.value.toString()) }

    fun detectedCountry(): FundingCountry? = FundingCountries.detected()

    fun countries(): List<FundingCountry> = FundingCountries.all()

    suspend fun brand(providerId: String): FundingProviderBrand = branding.brand(providerId)

    suspend fun candidates(intent: String): List<FundingCandidate> = runtime.candidates(intent)

    suspend fun session(intent: String): FundingSession? = runtime.session(intent)

    suspend fun progress(intent: String): FundingProgress? = runtime.progress(intent)

    suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<Unit> = runtime.requestQuote(intent, ask)

    suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean> =
        runtime.selectProvider(intent, providerId, quoteId)

    suspend fun cancel(intent: String): Result<Boolean> = runtime.cancel(intent)

    /** The provider's screen has done its part once the payment is seen or the session is over. */
    suspend fun paymentMoved(intent: String): Boolean {
        val session = runtime.session(intent) ?: return true
        if (!session.stage.isOpen) return true

        return runtime.progress(intent)?.steps.orEmpty().any { it.reachedAtMs != null && it.step != FundingStep.STARTED }
    }

    fun quoteRows(intent: String): Flow<FundingQuoteRow> = runtime.quoteRows(intent)

    fun sessionChanges(intent: String): Flow<Unit> = runtime.sessionChanges().filter { it == intent }.map { }
}
