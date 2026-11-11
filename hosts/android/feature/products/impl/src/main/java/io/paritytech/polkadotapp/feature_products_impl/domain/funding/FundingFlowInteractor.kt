package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.ProductId
import uniffi.truapi.U128
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
        return FundingCash(precision = asset.precision)
    }

    suspend fun spendable(cash: FundingCash): BigDecimal? =
        balance.spendable()?.let { cash.decimal(it.value.toString()) }


    suspend fun brand(providerId: ProductId): FundingProviderBrand = branding.brand(providerId)

    /** What the overlay offers, as the core last answered: the providers, and for the selection the countries and limits. */
    fun options(intent: String): Flow<FundingOptions?> = runtime.options(intent)

    suspend fun selectOptions(intent: String, selection: FundingSelection) = runtime.selectOptions(intent, selection)

    /** Session [intent] with its progress as the core holds it, `null` until the core announces it. */
    fun session(intent: String): Flow<FundingBookEntry?> = runtime.sessions.map { it[intent] }.distinctUntilChanged()

    /** Ended session [intent] as the core summarises it, or `null` when the core holds no session by that id. */
    suspend fun endedSummary(intent: String): FundingSessionSummary? =
        runtime.summary(intent).logFailure("Funding summary read failed").getOrNull()

    /** Reports [selection] first, so the quotes of [amount] are asked on it. */
    suspend fun requestQuote(intent: String, selection: FundingSelection, amount: U128): Result<ULong?> {
        runtime.selectOptions(intent, selection)
        return runtime.requestQuote(intent, amount)
    }

    suspend fun startWhenQuoted(intent: String, requestId: ULong) = runtime.startWhenQuoted(intent, requestId)

    suspend fun cancelAutoStart(intent: String) = runtime.cancelAutoStart(intent)

    suspend fun amountIssue(intent: String, amount: U128, spendable: U128?): FundingAmountIssue? =
        runtime.amountIssue(intent, amount, spendable).logFailure("Funding amount check failed").getOrNull()

    suspend fun selectProvider(intent: String, providerId: ProductId, quoteId: String): Result<Boolean> =
        runtime.selectProvider(intent, providerId, quoteId)

    suspend fun cancel(intent: String): Result<CancelOutcome> = runtime.cancel(intent)

    suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation> =
        runtime.confirmCancel(intent, terms)

    fun quoteLists(intent: String): Flow<FundingQuoteList?> = runtime.quoteLists(intent)
}
