package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
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

/** The amounts the providers take, as the core works them out; `null` where a bound is not known. */
data class FundingLimits(
    val minimum: BigDecimal?,
    val maximum: BigDecimal?,
)

/**
 * What the user has chosen in one funding sheet, with the core's options for it and the quotes of its latest ask, as
 * the funding service reports them. The core decides the rails, the asset an ask names, who is asked and whether the
 * amount can go ahead; this holds the user's picks and reads those facts for the screens.
 */
data class FundingFlowState(
    val direction: FundingDirection,
    val cash: FundingCash,
    val format: FundingCashFormat,
    val spendable: BigDecimal?,
    /** What the core offers for the selection on screen; `null` until it has said. */
    val options: FundingOptions?,
    val rail: FundingRail,
    /** Whether the user picked [rail], so the core's default no longer moves it. */
    val railChosen: Boolean,
    val amountText: String,
    val country: FundingCountry?,
    val network: String?,
    val asset: String?,
    val chosenProviderId: ProductId?,
    val rows: Map<ProductId, FundingQuoteState>,
    /** The quotes on screen before the same amount was asked again, shown greyed while the new ones arrive. */
    val staleQuotes: Map<ProductId, FundingQuote>,
    /** The core's list for the latest ask; `null` from a new ask until its first list arrives. */
    val list: FundingQuoteList?,
    /** An ask waits for its first list. */
    val quoting: Boolean,
    /** Why the typed amount cannot go ahead, as the core last answered for it. */
    val amountIssue: FundingAmountIssue?,
) {
    companion object {
        fun initial(
            direction: FundingDirection,
            cash: FundingCash,
            format: FundingCashFormat,
            spendable: BigDecimal?,
            options: FundingOptions?,
            amount: BigDecimal?,
            country: FundingCountry?,
        ) = FundingFlowState(
            direction = direction,
            cash = cash,
            format = format,
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
            staleQuotes = emptyMap(),
            list = null,
            quoting = false,
            amountIssue = null,
        )

        fun textFor(value: BigDecimal): String =
            value.setScale(2, RoundingMode.DOWN).stripTrailingZeros().toPlainString()
    }

    /** The typed amount in balance units, rounded down; `null` for text that is not a plain decimal amount. */
    val units: U128?
        get() = amountText.takeIf(AMOUNT_TEXT::matches)?.toBigDecimalOrNull()?.let(cash::units)

    val amount: BigDecimal
        get() = units?.let(cash::decimal) ?: BigDecimal.ZERO

    val candidates: List<FundingCandidate>
        get() = options?.candidates.orEmpty()

    val availableRails: Set<FundingRail>
        get() = options?.rails.orEmpty().toSet()

    /** The providers asked for the ask on screen, in the order the core lists them. */
    val servingCandidates: List<FundingCandidate>
        get() = rows.keys.mapNotNull { id -> candidates.firstOrNull { it.providerId == id } }

    val canContinue: Boolean
        get() = amount.signum() > 0 && amountIssue == null && rail in availableRails

    val amountLimits: FundingLimits
        get() = options?.limits.toLimits()

    /** The asset a card or bank ask names, as the core resolves it for the user's currency. */
    val fiatAsset: String?
        get() = options?.asset.takeIf { rail != FundingRail.CRYPTO }

    /** What [quote]'s provider-side figures are counted in: the asked asset, at the decimals the quote states. */
    fun unitOf(quote: FundingQuote): FundingAssetUnit =
        FundingAssetUnit.of(options?.asset ?: DEFAULT_ASSET, quote.assetDecimals.toInt())

    val quotedProviders: List<Pair<ProductId, FundingQuote>>
        get() = rows.mapNotNull { (providerId, state) -> state.quote?.let { providerId to it } }

    /** The user's choice while its quote stands, otherwise the one the core marks best. */
    val selectedProviderId: ProductId?
        get() = chosenProviderId?.takeIf { rows[it]?.quote != null } ?: list?.best?.takeIf { rows[it]?.quote != null }

    val selectedQuote: FundingQuote?
        get() = selectedProviderId?.let { rows[it]?.quote }

    val best: ProductId?
        get() = list?.best

    val refreshAt: Instant?
        get() = list?.let { Instant.fromEpochMilliseconds(it.refreshAtMs.toLong()) }

    /** Asked and waiting: the ask's first list has not come, or a provider on it has not answered. */
    val isQuoting: Boolean
        get() = selectedQuote == null && (quoting || list?.complete == false)

    /** The amount of the list on screen, in balance units. */
    val askedAmount: U128?
        get() = list?.amount

    val allAnswered: Boolean
        get() = list?.complete == true && rows.isNotEmpty()

    val noProviderQuoted: Boolean
        get() = allAnswered && quotedProviders.isEmpty()

    /** Why no provider would quote, for the summary screen, when their refusals say the amount is outside what they take. */
    val unavailableIssue: FundingAmountIssue?
        get() = list?.issue

    /** The ids of the crypto networks a provider routes. */
    val networks: List<String>
        get() = options?.networks.orEmpty().map { it.network }

    /** The least [networkId] takes, as the core says. */
    fun minimum(networkId: String): BigDecimal? =
        options?.networks?.firstOrNull { it.network == networkId }?.limits?.min?.let(cash::decimal)

    /** The tokens the core offers on the network picked, or on any before one is. */
    val tokens: List<FundingToken>
        get() = options?.tokens.orEmpty().map(::FundingToken)

    /** What the core knows about each country on this rail and asset, by ISO code. */
    val countrySupport: Map<String, FundingCountrySupport>
        get() = options?.countries.orEmpty().associate { it.country.uppercase(Locale.ROOT) to it.support }

    /** Countries the core says no provider on this rail and asset takes payment from. */
    val unsupportedCountries: Set<String>
        get() = countrySupport.filterValues { it is FundingCountrySupport.Unsupported }.keys

    /** The least each supported country takes, where a provider showed one. */
    val countryMinimums: Map<String, BigDecimal>
        get() = countrySupport.mapNotNull { (code, support) ->
            (support as? FundingCountrySupport.Supported)?.min?.let { code to cash.decimal(it) }
        }.toMap()

    /** The amount to ask quotes for: the typed one, once the asset is known: the token picked for crypto, else the one the core names. */
    val askAmount: U128?
        get() {
            val asked = if (rail == FundingRail.CRYPTO) asset else options?.asset
            return units?.takeIf { amount.signum() > 0 && asked != null }
        }

    fun withRail(newRail: FundingRail): FundingFlowState =
        if (newRail == rail) copy(railChosen = true) else copy(rail = newRail, railChosen = true, network = null, asset = null)

    fun withAmountText(text: String): FundingFlowState = copy(amountText = text)

    fun withCountry(newCountry: FundingCountry): FundingFlowState = copy(country = newCountry)

    fun withNetwork(id: String): FundingFlowState = copy(network = id, asset = null)

    fun withAsset(symbol: String): FundingFlowState = copy(asset = symbol)

    fun withChosenProvider(providerId: ProductId): FundingFlowState = copy(chosenProviderId = providerId)

    fun withAmountIssue(issue: FundingAmountIssue?): FundingFlowState = copy(amountIssue = issue)

    /**
     * The quotes of the latest ask as the service holds them. Until the ask's first list, the providers that last
     * quoted the same amount read pending with their quotes greyed.
     */
    fun withQuotes(newList: FundingQuoteList?, stale: Map<ProductId, FundingQuote>, asking: Boolean): FundingFlowState = copy(
        rows = newList?.rows?.associate { it.providerId to it.state } ?: stale.mapValues { FundingQuoteState.Pending },
        staleQuotes = stale,
        list = newList,
        quoting = asking,
    )

    /**
     * The rail, asset, network and country on screen, as the core is told of them, with the country's currency from
     * the platform's locale data.
     */
    val selection: FundingSelection
        get() = FundingSelection(
            rail = rail,
            asset = asset,
            network = network,
            country = country?.code,
            currency = country?.currencyCode,
        )

    /** The core's options for the selection on screen; before the user picks a rail, its default rail too. */
    fun withOptions(newOptions: FundingOptions): FundingFlowState {
        val updated = copy(options = newOptions)
        return if (railChosen || newOptions.defaultRail == rail) {
            updated
        } else {
            updated.copy(rail = newOptions.defaultRail, network = null, asset = null)
        }
    }

    private fun FundingAmountLimits?.toLimits() = FundingLimits(
        minimum = this?.min?.let(cash::decimal),
        maximum = this?.max?.let(cash::decimal),
    )
}

// The asset a quote is read in before the core named one.
private const val DEFAULT_ASSET = "USD"

private val AMOUNT_TEXT = Regex("""\d*\.?\d*""")

/** The first rail the core routes, crypto first, or card when it routes none. */
private val FundingOptions?.defaultRail: FundingRail
    get() = this?.rails?.firstOrNull() ?: FundingRail.CARD
