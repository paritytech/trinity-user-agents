package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import uniffi.truapi.fundingAmountIssue
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
 * What the user has chosen in one funding overlay, with the core's options for it and its latest quote list. The
 * core decides the rails, the asset an ask names, who is asked and whether the amount can go ahead; this holds the
 * user's picks and reads those facts for the screens.
 */
data class FundingFlowState(
    val direction: FundingDirection,
    val cash: FundingCash,
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
    val ask: FundingQuoteAsk?,
    /** The quotes on screen before the same ask went out again, shown greyed while the new ones arrive. */
    val staleQuotes: Map<ProductId, FundingQuote>,
    /** The core's latest list for [ask]; `null` from a new ask until its first list arrives. */
    val list: FundingQuoteList?,
    /**
     * The id the core returned for [ask]; only lists for it are shown. `null` while no ask is current or its id is
     * still on its way.
     */
    val requestId: ULong?,
    /** Counts the asks and drops, so the id of an ask replaced while on its way is not taken; see [requested]. */
    val askNumber: Int,
    /** The newest list that arrived while [ask]'s id was on its way; shown if the id turns out to be its. */
    val earlyList: FundingQuoteList?,
) {
    companion object {
        fun initial(
            direction: FundingDirection,
            cash: FundingCash,
            spendable: BigDecimal?,
            options: FundingOptions?,
            amount: BigDecimal?,
            country: FundingCountry?,
        ) = FundingFlowState(
            direction = direction,
            cash = cash,
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
            ask = null,
            staleQuotes = emptyMap(),
            list = null,
            requestId = null,
            askNumber = 0,
            earlyList = null,
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

    /** Why the typed amount cannot go ahead, as the core says, including every provider refusing this amount. */
    val amountIssue: FundingAmountIssue?
        get() {
            val units = units ?: return null
            val limits = options?.limits ?: FundingAmountLimits(min = null, max = null)
            return fundingAmountIssue(direction, limits, list, units, spendable?.let(::unitsOf))
        }

    val amountLimits: FundingLimits
        get() = options?.limits.toLimits()

    /** The asset a card or bank ask names, as the core resolves it for the user's currency. */
    val fiatAsset: String?
        get() = options?.asset.takeIf { rail != FundingRail.CRYPTO }

    /** What [quote]'s provider-side figures are counted in: the asked asset, at the decimals the quote states. */
    fun unitOf(quote: FundingQuote): FundingAssetUnit =
        FundingAssetUnit.of(ask?.asset ?: options?.asset ?: DEFAULT_ASSET, quote.assetDecimals.toInt())

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
        get() = selectedQuote == null && ask != null && (list == null || rows.values.any { it.isPending })

    val allAnswered: Boolean
        get() = list != null && rows.isNotEmpty() && rows.values.none { it.isPending }

    val noProviderQuoted: Boolean
        get() = allAnswered && quotedProviders.isEmpty()

    /** Why no provider would quote, for the summary screen, when the amount is outside what they take. */
    val unavailableIssue: FundingAmountIssue?
        get() = if (noProviderQuoted) amountIssue else null

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

    fun currentAsk(): FundingQuoteAsk? {
        val units = units?.takeIf { amount.signum() > 0 } ?: return null

        val askedAsset = when (rail) {
            FundingRail.CRYPTO -> asset
            FundingRail.CARD, FundingRail.BANK -> fiatAsset
        } ?: return null

        return FundingQuoteAsk(
            direction = direction,
            rail = rail,
            asset = askedAsset,
            network = if (rail == FundingRail.CRYPTO) network else null,
            amount = units,
            country = country?.code,
        )
    }

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

    /**
     * A new ask goes out: its rows read pending until the core's first list for it names who is asked. No list shows
     * until the core returns the ask's id to [requested] with the returned [askNumber].
     */
    fun asking(newAsk: FundingQuoteAsk): FundingFlowState = copy(
        requestId = null,
        askNumber = askNumber + 1,
        earlyList = null,
        ask = newAsk,
        rows = rows.mapValues { FundingQuoteState.Pending },
        staleQuotes = if (newAsk == ask) quotedProviders.toMap() else emptyMap(),
        list = null,
    )

    /**
     * The core returned [id] for ask [number]: its lists show from now on, starting with one that arrived first.
     * Ignored when a later ask or a drop replaced that ask.
     */
    fun requested(id: ULong, number: Int): FundingFlowState {
        if (number != askNumber || ask == null) return this

        val early = earlyList
        val taken = copy(requestId = id, earlyList = null)
        return if (early?.requestId == id) taken.receiving(early) else taken
    }

    /**
     * The core's latest list, if it answers the current ask; while the ask's id is on its way, the newest list is
     * kept for [requested] instead. A row still pending after the same ask went out again keeps its last quote
     * greyed until it answers.
     */
    fun receiving(list: FundingQuoteList): FundingFlowState {
        val current = requestId
        if (current == null) {
            val awaitsId = ask != null
            return if (awaitsId && list.requestId >= (earlyList?.requestId ?: 0u)) copy(earlyList = list) else this
        }
        if (list.requestId != current) return this

        val newRows = list.rows.associate { it.providerId to it.state }
        val stale = newRows
            .filterValues { it.isPending }
            .mapNotNull { (providerId, _) -> (rows[providerId]?.quote ?: staleQuotes[providerId])?.let { providerId to it } }
            .toMap()

        return copy(rows = newRows, staleQuotes = stale, list = list)
    }

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
            updated.dropped().copy(rail = newOptions.defaultRail, network = null, asset = null)
        }
    }

    /** No quotes, and no list for any ask made so far, including one whose id is still on its way. */
    private fun dropped(): FundingFlowState = copy(
        rows = emptyMap(),
        ask = null,
        list = null,
        requestId = null,
        askNumber = askNumber + 1,
        earlyList = null,
    )

    private fun unitsOf(value: BigDecimal): U128 = cash.units(value)

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
