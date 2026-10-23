package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import java.math.BigDecimal
import java.math.RoundingMode
import java.util.Locale
import kotlin.time.Instant
import uniffi.truapi.FundingCountry as CoreFundingCountry

/** Why the typed amount cannot go ahead. */
sealed interface FundingAmountIssue {
    data class BelowMinimum(val minimum: BigDecimal) : FundingAmountIssue

    data class AboveMaximum(val maximum: BigDecimal) : FundingAmountIssue

    data object NotEnoughBalance : FundingAmountIssue
}

/** The amounts the providers take, as the core works them out; `null` where a bound is not known. */
data class FundingLimits(
    val minimum: BigDecimal?,
    val maximum: BigDecimal?,
)

/**
 * What the user has chosen in one funding overlay and what the providers quoted for it. Every transition is a
 * function here, so the screens only read the derived figures.
 */
data class FundingFlowState(
    val direction: FundingDirection,
    val cash: FundingCash,
    val spendable: BigDecimal?,
    val candidates: List<FundingCandidate>,
    val rail: FundingRail,
    val amountText: String,
    val country: FundingCountry?,
    val network: String?,
    val asset: String?,
    val chosenProviderId: String?,
    val rows: Map<String, FundingQuoteState>,
    val ask: FundingQuoteAsk?,
    /** The quotes on screen before the same ask went out again, shown greyed while the new ones arrive. */
    val staleQuotes: Map<String, FundingQuote> = emptyMap(),
    /** The provider the core marks best among the quotes. */
    val best: String? = null,
    /** When the core asks every provider again. */
    val refreshAt: Instant? = null,
    /** What the core knows about each country on this rail and asset, by ISO code. */
    val countrySupport: Map<String, FundingCountrySupport> = emptyMap(),
    /**
     * The earliest quote request whose lists still belong on screen; a list for an earlier one is dropped. The core
     * numbers requests in order, so a new ask moves this past every request made so far.
     */
    val requestId: ULong? = null,
    /** What the core says this rail, asset, network and country take. */
    val amountLimits: FundingLimits = FundingLimits(minimum = null, maximum = null),
    /** The least each crypto network takes, as the core says. */
    val networkMinimums: Map<String, BigDecimal> = emptyMap(),
) {
    companion object {
        private const val DEFAULT_FIAT = "USD"

        fun initial(
            direction: FundingDirection,
            cash: FundingCash,
            spendable: BigDecimal?,
            candidates: List<FundingCandidate>,
            amount: BigDecimal?,
            country: FundingCountry?,
        ) = FundingFlowState(
            direction = direction,
            cash = cash,
            spendable = spendable,
            candidates = candidates,
            rail = defaultRail(candidates, direction),
            amountText = amount?.let(::textFor).orEmpty(),
            country = country,
            network = null,
            asset = null,
            chosenProviderId = null,
            rows = emptyMap(),
            ask = null,
        )

        fun textFor(value: BigDecimal): String =
            value.setScale(2, RoundingMode.DOWN).stripTrailingZeros().toPlainString()

        private fun defaultRail(candidates: List<FundingCandidate>, direction: FundingDirection): FundingRail {
            val served = FUNDING_RAIL_ORDER.filter { rail -> candidates.any { it.serves(rail, direction) } }
            return if (FundingRail.CARD in served) FundingRail.CARD else served.firstOrNull() ?: FundingRail.CARD
        }
    }

    val amount: BigDecimal
        get() = amountText.toBigDecimalOrNull() ?: BigDecimal.ZERO

    val availableRails: Set<FundingRail>
        get() = FUNDING_RAIL_ORDER.filter { rail -> candidates.any { it.serves(rail, direction) } }.toSet()

    val servingCandidates: List<FundingCandidate>
        get() = candidates.filter { it.serves(rail, direction) }

    val canContinue: Boolean
        get() = amount.signum() > 0 && amountIssue == null && rail in availableRails

    val amountIssue: FundingAmountIssue?
        get() {
            if (direction == FundingDirection.OUT && spendable != null && amount > spendable) {
                return FundingAmountIssue.NotEnoughBalance
            }

            val (minimum, maximum) = amountLimits
            return when {
                amount.signum() > 0 && minimum != null && amount < minimum -> FundingAmountIssue.BelowMinimum(minimum)

                maximum != null && amount > maximum -> FundingAmountIssue.AboveMaximum(maximum)

                else -> null
            }
        }

    /** The payment country's currency when a provider takes it, otherwise the first one a provider lists. */
    val fiatAsset: String
        get() {
            val listed = servingCandidates.flatMap { it.routes(rail, direction) }.flatMap { it.assets }
            val currency = country?.currencyCode
            if (currency != null && (listed.isEmpty() || listed.any { it.equals(currency, ignoreCase = true) })) {
                return currency
            }
            return listed.firstOrNull() ?: currency ?: DEFAULT_FIAT
        }

    /** What [quote]'s provider-side figures are counted in: the asked asset, at the decimals the quote states. */
    fun unitOf(quote: FundingQuote): FundingAssetUnit =
        FundingAssetUnit.of(ask?.asset ?: fiatAsset, quote.assetDecimals.toInt())

    val quotedProviders: List<Pair<String, FundingQuote>>
        get() = rows.mapNotNull { (providerId, state) -> state.quote?.let { providerId to it } }

    /** The user's choice while its quote stands, otherwise the one the core marks best. */
    val selectedProviderId: String?
        get() = chosenProviderId?.takeIf { rows[it]?.quote != null } ?: best?.takeIf { rows[it]?.quote != null }

    val selectedQuote: FundingQuote?
        get() = selectedProviderId?.let { rows[it]?.quote }

    val isQuoting: Boolean
        get() = rows.values.any { it.isPending } && selectedQuote == null

    val allAnswered: Boolean
        get() = rows.isNotEmpty() && rows.values.none { it.isPending }

    val noProviderQuoted: Boolean
        get() = allAnswered && quotedProviders.isEmpty()

    /** Why no provider would quote, for the summary screen, when the amount is outside what they take. */
    val unavailableIssue: FundingAmountIssue?
        get() = if (noProviderQuoted) amountIssue else null

    val cryptoRoutes: List<FundingRoute>
        get() = candidates.flatMap { it.routes(FundingRail.CRYPTO, direction) }

    val networks: List<FundingNetwork>
        get() = cryptoRoutes.flatMap { it.networks.orEmpty() }.distinct().map(::FundingNetwork)

    /** The least [networkId] takes, as the core says. */
    fun minimum(networkId: String): BigDecimal? = networkMinimums[networkId]

    val tokens: List<FundingToken>
        get() = cryptoRoutes
            .filter { route -> network == null || route.networks?.contains(network) ?: true }
            .flatMap { it.assets }
            .distinct()
            .map(::FundingToken)

    /** Countries the core says no provider on this rail and asset takes payment from. */
    val unsupportedCountries: Set<String>
        get() = countrySupport.filterValues { it is FundingCountrySupport.Unsupported }.keys

    /** The least each supported country takes, where a provider showed one. */
    val countryMinimums: Map<String, BigDecimal>
        get() = countrySupport.mapNotNull { (code, support) ->
            (support as? FundingCountrySupport.Supported)?.min?.let { code to cash.decimal(it) }
        }.toMap()

    fun currentAsk(): FundingQuoteAsk? {
        if (amount.signum() <= 0) return null

        val askedAsset = when (rail) {
            FundingRail.CRYPTO -> asset ?: return null
            FundingRail.CARD, FundingRail.BANK -> fiatAsset
        }

        return FundingQuoteAsk(
            direction = direction,
            rail = rail,
            asset = askedAsset,
            network = if (rail == FundingRail.CRYPTO) network else null,
            amount = cash.units(amount),
            country = country?.code,
        )
    }

    fun withRail(newRail: FundingRail): FundingFlowState =
        if (newRail == rail) this else copy(rail = newRail, network = null, asset = null, rows = emptyMap(), ask = null)

    fun withAmountText(text: String): FundingFlowState = copy(amountText = text)

    fun withCountry(newCountry: FundingCountry): FundingFlowState = copy(country = newCountry)

    fun withNetwork(id: String): FundingFlowState = copy(network = id, asset = null)

    fun withAsset(symbol: String): FundingFlowState = copy(asset = symbol)

    fun withChosenProvider(providerId: String): FundingFlowState = copy(chosenProviderId = providerId)

    fun withCandidates(newCandidates: List<FundingCandidate>): FundingFlowState = copy(candidates = newCandidates)

    /** Every serving provider is asked again, so each row reads pending until its answer arrives. */
    fun asking(newAsk: FundingQuoteAsk): FundingFlowState = copy(
        requestId = requestId?.plus(1u),
        ask = newAsk,
        rows = servingCandidates.associate { it.providerId to FundingQuoteState.Pending },
        staleQuotes = if (newAsk == ask) quotedProviders.toMap() else emptyMap(),
        best = null,
        refreshAt = null,
    )

    /** Request [id] went out, so lists for any earlier request are no longer this screen's. */
    fun requested(id: ULong): FundingFlowState = copy(requestId = maxOf(requestId ?: id, id))

    /**
     * The core's latest list, unless it answers an earlier request than the one on screen. A row back to pending at
     * a refresh keeps its last quote greyed until it answers.
     */
    fun receiving(list: FundingQuoteList): FundingFlowState {
        val latest = requestId
        if (latest != null && list.requestId < latest) return this

        val newRows = list.rows.associate { it.providerId to it.state }
        val stale = newRows
            .filterValues { it.isPending }
            .mapNotNull { (providerId, _) -> (rows[providerId]?.quote ?: staleQuotes[providerId])?.let { providerId to it } }
            .toMap()

        return copy(
            requestId = list.requestId,
            rows = newRows,
            staleQuotes = stale,
            best = list.best,
            refreshAt = Instant.fromEpochMilliseconds(list.refreshAtMs.toLong()),
        )
    }

    /** The choice the amount limits are read for; an answer for another one is stale. */
    val limitsKey: FundingLimitsKey
        get() = FundingLimitsKey(rail = rail, asset = asset ?: fiatAsset, network = network, country = country?.code)

    fun withAmountLimits(limits: FundingLimits): FundingFlowState = copy(amountLimits = limits)

    fun withNetworkMinimums(minimums: Map<String, BigDecimal>): FundingFlowState = copy(networkMinimums = minimums)

    fun withCountrySupport(countries: List<CoreFundingCountry>): FundingFlowState =
        copy(countrySupport = countries.associate { it.country.uppercase(Locale.ROOT) to it.support })
}

/** The rail, asset, network and country an amount-limit lookup answers for. */
data class FundingLimitsKey(
    val rail: FundingRail,
    val asset: String,
    val network: String?,
    val country: String?,
)
