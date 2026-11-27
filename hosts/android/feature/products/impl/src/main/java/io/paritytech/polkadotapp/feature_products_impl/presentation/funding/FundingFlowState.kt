package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isPending
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import java.math.BigDecimal
import java.math.RoundingMode
import java.util.Locale
import kotlin.time.Instant

data class FundingLimits(
    val minimum: BigDecimal?,
    val maximum: BigDecimal?,
)

data class FundingFlowState(
    val direction: FundingDirection,
    val cashPrecision: Int,
    val spendable: BigDecimal?,
    val options: FundingOptions?,
    val rail: FundingRail,
    val railChosen: Boolean,
    val amountText: String,
    val country: FundingCountry?,
    val network: String?,
    val asset: String?,
    val chosenProviderId: ProductId?,
    val rows: Map<ProductId, FundingQuoteState>,
    val askedAmount: Balance?,
    val staleQuotes: Map<ProductId, FundingQuote>,
    val list: FundingQuoteList?,
    val amountIssue: FundingAmountIssue?,
) {
    companion object {
        fun initial(
            direction: FundingDirection,
            cashPrecision: Int,
            spendable: BigDecimal?,
            options: FundingOptions?,
            amount: BigDecimal?,
            country: FundingCountry?,
        ) = FundingFlowState(
            direction = direction,
            cashPrecision = cashPrecision,
            spendable = spendable,
            options = options,
            rail = options.defaultRail,
            railChosen = false,
            amountText = amount?.let(::textFor).orEmpty(),
            country = country,
            network = null,
            asset = null,
            chosenProviderId = null,
            rows = emptyMap(),
            askedAmount = null,
            staleQuotes = emptyMap(),
            list = null,
            amountIssue = null,
        )

        fun textFor(value: BigDecimal): String =
            value.setScale(2, RoundingMode.DOWN).stripTrailingZeros().toPlainString()
    }

    // Rounded down so the host never asks for more than the user typed.
    val units: Balance?
        get() = amountText.takeIf(AMOUNT_TEXT::matches)?.toBigDecimalOrNull()?.planksFromAmount(cashPrecision)

    val amount: BigDecimal
        get() = units?.let(::decimal) ?: BigDecimal.ZERO

    val candidates: List<FundingCandidate>
        get() = options?.candidates.orEmpty()

    val availableRails: Set<FundingRail>
        get() = options?.rails.orEmpty().toSet()

    val servingCandidates: List<FundingCandidate>
        get() = rows.keys.mapNotNull { id -> candidates.firstOrNull { it.providerId == id } }

    val canContinue: Boolean
        get() = amount.signum() > 0 && amountIssue == null && rail in availableRails

    val amountLimits: FundingLimits
        get() = options?.limits.toLimits()

    val fiatAsset: String?
        get() = options?.asset.takeIf { rail != FundingRail.CRYPTO }

    fun unitOf(quote: FundingQuote): FundingAssetUnit =
        FundingAssetUnit.of(options?.asset ?: DEFAULT_ASSET, quote.assetDecimals.toInt())

    val quotedProviders: List<Pair<ProductId, FundingQuote>>
        get() = rows.mapNotNull { (providerId, state) -> state.quote?.let { providerId to it } }

    val selectedProviderId: ProductId?
        get() = chosenProviderId?.takeIf { rows[it]?.quote != null } ?: list?.best?.takeIf { rows[it]?.quote != null }

    val selectedQuote: FundingQuote?
        get() = selectedProviderId?.let { rows[it]?.quote }

    val best: ProductId?
        get() = list?.best

    val refreshAt: Instant?
        get() = list?.let { Instant.fromEpochMilliseconds(it.refreshAtMs.toLong()) }

    val isQuoting: Boolean
        get() = selectedQuote == null && askedAmount != null && list?.complete != true

    val allAnswered: Boolean
        get() = list?.complete == true && rows.isNotEmpty()

    val noProviderQuoted: Boolean
        get() = allAnswered && quotedProviders.isEmpty()

    val unavailableIssue: FundingAmountIssue?
        get() = list?.issue

    val networks: List<String>
        get() = options?.networks.orEmpty().map { it.network }

    fun minimum(networkId: String): BigDecimal? =
        options?.networks?.firstOrNull { it.network == networkId }?.limits?.min?.let(::cashDecimal)

    val tokens: List<FundingToken>
        get() = options?.tokens.orEmpty().map(::FundingToken)

    val countrySupport: Map<String, FundingCountrySupport>
        get() = options?.countries.orEmpty().associate { it.country.uppercase(Locale.ROOT) to it.support }

    val unsupportedCountries: Set<String>
        get() = countrySupport.filterValues { it is FundingCountrySupport.Unsupported }.keys

    val countryMinimums: Map<String, BigDecimal>
        get() = countrySupport.mapNotNull { (code, support) ->
            (support as? FundingCountrySupport.Supported)?.min?.let(::cashDecimal)?.let { code to it }
        }.toMap()

    val askAmount: Balance?
        get() = units?.takeIf { amount.signum() > 0 && options?.asset != null }

    fun withRail(newRail: FundingRail): FundingFlowState =
        if (newRail == rail) {
            copy(railChosen = true)
        } else {
            dropped().copy(rail = newRail, railChosen = true, network = null, asset = null)
        }

    fun withAmountText(text: String): FundingFlowState = copy(amountText = text)

    fun withCountry(newCountry: FundingCountry): FundingFlowState = copy(country = newCountry)

    fun withNetwork(id: String): FundingFlowState = copy(network = id, asset = null)

    fun withAsset(symbol: String): FundingFlowState = copy(asset = symbol)

    fun withChosenProvider(providerId: ProductId): FundingFlowState = copy(chosenProviderId = providerId)

    fun withAmountIssue(issue: FundingAmountIssue?): FundingFlowState = copy(amountIssue = issue)

    fun asking(amount: Balance): FundingFlowState = copy(
        askedAmount = amount,
        rows = rows.mapValues { FundingQuoteState.Pending },
        staleQuotes = if (amount == askedAmount) quotedProviders.toMap() else emptyMap(),
        list = null,
    )

    fun receiving(newList: FundingQuoteList?): FundingFlowState {
        if (newList == null) return dropped()
        if (askedAmount == null) return this

        val newRows = newList.rows.associate { it.providerId to it.state }
        val stale = newRows
            .filterValues { it.isPending }
            .mapNotNull { (providerId, _) -> (rows[providerId]?.quote ?: staleQuotes[providerId])?.let { providerId to it } }
            .toMap()

        return copy(rows = newRows, staleQuotes = stale, list = newList)
    }

    val selection: FundingSelection
        get() = FundingSelection(
            rail = rail,
            asset = asset,
            network = network,
            country = country?.code,
            currency = country?.currencyCode,
        )

    fun withOptions(newOptions: FundingOptions): FundingFlowState {
        val updated = copy(options = newOptions)
        return if (railChosen || newOptions.defaultRail == rail) {
            updated
        } else {
            updated.dropped().copy(rail = newOptions.defaultRail, network = null, asset = null)
        }
    }

    private fun dropped(): FundingFlowState = copy(rows = emptyMap(), askedAmount = null, staleQuotes = emptyMap(), list = null)

    private fun FundingAmountLimits?.toLimits() = FundingLimits(
        minimum = this?.min?.let(::cashDecimal),
        maximum = this?.max?.let(::cashDecimal),
    )

    private fun decimal(units: Balance): BigDecimal = units.amountFromPlanks(cashPrecision).canonical()

    private fun cashDecimal(units: U128): BigDecimal? = units.toBalanceOrNull()?.let(::decimal)
}

val FUNDING_RAIL_ORDER: List<FundingRail> = listOf(FundingRail.CRYPTO, FundingRail.CARD, FundingRail.BANK)

private const val DEFAULT_ASSET = "USD"

private val AMOUNT_TEXT = Regex("""\d*\.?\d*""")

private val FundingOptions?.defaultRail: FundingRail
    get() = this?.rails?.firstOrNull() ?: FundingRail.CARD
