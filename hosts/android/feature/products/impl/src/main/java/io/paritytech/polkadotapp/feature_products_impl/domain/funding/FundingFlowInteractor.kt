package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import java.math.BigDecimal
import javax.inject.Inject

class FundingFlowInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val balance: FundingBalance,
    private val runtime: FundingRuntime,
    private val branding: FundingProviderBranding,
    private val countryDetector: DeviceCountryDetector,
) {
    suspend fun cash(): FundingCash {
        val asset = chainAssetProvider.asset()
        return FundingCash(symbol = asset.symbol, precision = asset.precision)
    }

    suspend fun spendable(cash: FundingCash): BigDecimal? =
        balance.spendable()?.let { cash.decimal(it.value.toString()) }

    /** The country the device seems to be in, which quotes go by until the user picks one. */
    suspend fun detectedCountry(): FundingCountry? = countryDetector.detect()?.let { FundingCountries.country(it) }

    fun countries(): List<FundingCountry> = FundingCountries.all()

    suspend fun brand(providerId: String): FundingProviderBrand = branding.brand(providerId)

    /** What the overlay offers, as the core last pushed it: the providers, and for the selection the countries and limits. */
    fun options(intent: String): Flow<FundingOptions?> = runtime.options(intent)

    suspend fun selectOptions(intent: String, selection: FundingSelection) = runtime.selectOptions(intent, selection)

    fun session(intent: String): FundingSession? = runtime.session(intent)

    fun progress(intent: String): FundingProgress? = runtime.progress(intent)

    suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<ULong> = runtime.requestQuote(intent, ask)

    suspend fun stopQuotes(intent: String) = runtime.stopQuotes(intent)

    suspend fun selectProvider(intent: String, providerId: String, quoteId: String?): Result<Boolean> =
        runtime.selectProvider(intent, providerId, quoteId)

    suspend fun cancel(intent: String): Result<CancelOutcome> = runtime.cancel(intent)

    suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation> =
        runtime.confirmCancel(intent, terms)

    /** The provider's screen has done its part once the payment is seen or the session is over. */
    fun paymentMoved(intent: String): Boolean = runtime.paymentMoved(intent)

    fun quoteLists(intent: String): Flow<FundingQuoteList> = runtime.quoteLists(intent)

    fun sessionChanges(intent: String): Flow<Unit> = runtime.sessionChanges().filter { it == intent }.map { }
}
