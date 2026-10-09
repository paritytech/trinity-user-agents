package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import java.math.BigDecimal
import java.math.RoundingMode
import java.util.Locale
import kotlin.time.Instant

/** Why the typed amount cannot go ahead. */
sealed interface FundingAmountIssue {
    data class BelowMinimum(val minimum: BigDecimal) : FundingAmountIssue

    data class AboveMaximum(val maximum: BigDecimal) : FundingAmountIssue

    data object NotEnoughBalance : FundingAmountIssue
}

/** The lowest minimum and highest maximum the providers have refused with; `null` until one has. */
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

    /** Learned by the core from refusals and kept on the candidates, so none shows before a provider refused. */
    val learnedLimits: FundingLimits
        get() {
            val limits = servingCandidates
                .flatMap { it.limits }
                .filter { limit ->
                    limit.rail == rail &&
                        (asset == null || limit.asset.equals(asset, ignoreCase = true)) &&
                        (network == null || limit.network == null || limit.network == network)
                }

            return FundingLimits(
                minimum = limits.mapNotNull { limit -> limit.min?.let(cash::decimal) }.minOrNull(),
                maximum = limits.mapNotNull { limit -> limit.max?.let(cash::decimal) }.maxOrNull(),
            )
        }

    val amountIssue: FundingAmountIssue?
        get() {
            if (direction == FundingDirection.OUT && spendable != null && amount > spendable) {
                return FundingAmountIssue.NotEnoughBalance
            }
            refusalForCurrentAmount?.let { return it }

            val limits = learnedLimits
            return when {
                amount.signum() > 0 && limits.minimum != null && amount < limits.minimum ->
                    FundingAmountIssue.BelowMinimum(limits.minimum)

                limits.maximum != null && amount > limits.maximum -> FundingAmountIssue.AboveMaximum(limits.maximum)

                else -> null
            }
        }

    /** Every provider asked about this exact amount refused it on a limit. */
    val refusalForCurrentAmount: FundingAmountIssue?
        get() {
            val asked = ask ?: return null
            if (asked.amount != cash.units(amount) || rows.isEmpty()) return null
            if (rows.values.any { it.isPending || it.quote != null }) return null

            val refusals = rows.values.mapNotNull { it.refusal }
            val minimum = refusals.filterIsInstance<FundingQuoteRefusal.BelowMinimum>().minOfOrNull { cash.decimal(it.min) }
            if (minimum != null) return FundingAmountIssue.BelowMinimum(minimum)

            val maximum = refusals.filterIsInstance<FundingQuoteRefusal.AboveMaximum>().maxOfOrNull { cash.decimal(it.max) }
            return maximum?.let { FundingAmountIssue.AboveMaximum(it) }
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

    /** The asset the quote's provider-side figures are counted in. */
    val quoteUnit: FundingAssetUnit
        get() = FundingAssetUnit.of(ask?.asset ?: fiatAsset)

    val quotedProviders: List<Pair<String, FundingQuote>>
        get() = rows.mapNotNull { (providerId, state) -> state.quote?.let { providerId to it } }

    /** The cheapest quote: least to pay for value in, most received for value out. */
    val bestProviderId: String?
        get() {
            val unit = quoteUnit
            return when (direction) {
                FundingDirection.IN -> quotedProviders.minByOrNull { unit.decimal(it.second.sendAmount) }?.first
                FundingDirection.OUT -> quotedProviders.maxByOrNull { unit.decimal(it.second.receiveAmount) }?.first
            }
        }

    val selectedProviderId: String?
        get() = chosenProviderId?.takeIf { rows[it]?.quote != null } ?: bestProviderId

    val selectedQuote: FundingQuote?
        get() = selectedProviderId?.let { rows[it]?.quote }

    val isQuoting: Boolean
        get() = rows.values.any { it.isPending } && selectedQuote == null

    val quoteExpiry: Instant?
        get() = quotedProviders.mapNotNull { it.second.expiresAt }.minOrNull()
            ?.let { Instant.fromEpochMilliseconds(it.toLong()) }

    val allAnswered: Boolean
        get() = rows.isNotEmpty() && rows.values.none { it.isPending }

    val noProviderQuoted: Boolean
        get() = allAnswered && quotedProviders.isEmpty()

    /** What every provider said when none would quote, for the summary screen. */
    val unavailableIssue: FundingAmountIssue?
        get() = if (noProviderQuoted) refusalForCurrentAmount else null

    val cryptoRoutes: List<FundingRoute>
        get() = candidates.flatMap { it.routes(FundingRail.CRYPTO, direction) }

    val networks: List<FundingNetwork>
        get() = cryptoRoutes.flatMap { it.networks.orEmpty() }.distinct().map(::FundingNetwork)

    /** The lowest minimum a provider has refused with on [networkId]. */
    fun minimum(networkId: String): BigDecimal? = candidates
        .flatMap { it.limits }
        .filter { it.rail == FundingRail.CRYPTO && it.network == networkId }
        .mapNotNull { limit -> limit.min?.let(cash::decimal) }
        .minOrNull()

    val tokens: List<FundingToken>
        get() = cryptoRoutes
            .filter { route -> network == null || route.networks?.contains(network) ?: true }
            .flatMap { it.assets }
            .distinct()
            .map(::FundingToken)

    /** Countries no provider serving this rail takes payment from: one provider refusing is not enough. */
    val unsupportedCountries: Set<String>
        get() {
            val serving = servingCandidates
            if (serving.isEmpty()) return emptySet()

            val refused = serving
                .map { candidate ->
                    candidate.unsupported
                        .filter { it.rail == rail }
                        .mapNotNull { it.country?.uppercase(Locale.ROOT) }
                        .toSet()
                }
                .reduce { all, next -> all intersect next }
                .toMutableSet()

            val askedCountry = ask?.country
            val everyRowRefusedCountry = rows.isNotEmpty() &&
                rows.values.all { it.refusal == FundingQuoteRefusal.CountryUnsupported }
            if (everyRowRefusedCountry && askedCountry != null) refused += askedCountry.uppercase(Locale.ROOT)

            return refused
        }

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
    fun asking(newAsk: FundingQuoteAsk): FundingFlowState =
        copy(ask = newAsk, rows = servingCandidates.associate { it.providerId to FundingQuoteState.Pending })

    fun receiving(row: FundingQuoteRow): FundingFlowState = copy(rows = rows + (row.providerId to row.state))
}
