package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FUNDING_RAIL_ORDER
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAmountIssue
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isPending
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import java.math.BigInteger
import java.math.RoundingMode
import kotlin.math.ceil
import kotlin.math.max
import kotlin.time.Instant

private val PRESETS = listOf(BigDecimal("10"), BigDecimal("50"), BigDecimal("100"))

fun FundingFlowState.toAmountUiState(): FundingAmountUiState {
    val withdraw = direction == FundingDirection.OUT
    val available = availableRails

    return FundingAmountUiState(
        isWithdraw = withdraw,
        rails = FUNDING_RAIL_ORDER.map { FundingRailTab(it, selected = it == rail, enabled = it in available) }.toImmutableList(),
        amount = cash.figure(amount),
        digitCount = amountText.count { it.isDigit() },
        hasAmount = amount.signum() > 0,
        available = spendable?.takeIf { withdraw }?.let(cash::label),
        note = amountNote(),
        presets = if (withdraw) persistentListOf() else PRESETS.map { FundingPreset(it, cash.figure(it)) }.toImmutableList(),
        canContinue = canContinue,
    )
}

private fun FundingFlowState.amountNote(): FundingAmountNote? = when (val issue = amountIssue) {
    is FundingAmountIssue.BelowMinimum -> FundingAmountNote.BelowMinimum(cash.label(issue.minimum))
    is FundingAmountIssue.AboveMaximum -> FundingAmountNote.AboveMaximum(cash.label(issue.maximum))
    FundingAmountIssue.NotEnoughBalance -> FundingAmountNote.NotEnough(cash.symbol)
    null -> learnedLimits.minimum?.let {
        FundingAmountNote.Minimum(cash.label(it), withdraw = direction == FundingDirection.OUT)
    }
}

private const val SECONDS_PER_MINUTE = 60.0
private const val MINUTES_PER_HOUR = 60
private const val HOURS_PER_DAY = 24
private const val RATE_SCALE = 4

fun FundingFlowState.toSummaryUiState(
    brands: Map<String, FundingProviderBrand>,
    start: FundingStartState,
): FundingSummaryUiState {
    val quote = selectedQuote
    val inbound = direction == FundingDirection.IN

    return FundingSummaryUiState(
        isWithdraw = !inbound,
        rail = rail,
        headline = quote?.let { quoteUnit.format(if (inbound) it.sendAmount else it.receiveAmount) },
        isQuoting = isQuoting,
        showCountry = rail != FundingRail.CRYPTO,
        country = country?.toUi(refused = false, selected = true),
        provider = selectedProviderId?.let { brands.brandOf(it) },
        payout = quote?.let { cash.label(cash.decimal(if (inbound) it.receiveAmount else it.sendAmount)) },
        eta = quote?.etaSecs?.let(::etaOf),
        bankRateNoteSymbol = cash.symbol.takeIf { rail == FundingRail.BANK && inbound },
        problem = summaryProblem(start),
        canStart = quote != null && !start.isStarting && !start.started,
        isStarting = start.isStarting,
    )
}

private fun FundingFlowState.summaryProblem(start: FundingStartState): FundingSummaryProblem? = when {
    start.failed -> FundingSummaryProblem.StartFailed
    !noProviderQuoted -> null
    else -> when (val issue = unavailableIssue) {
        is FundingAmountIssue.BelowMinimum -> FundingSummaryProblem.Minimum(cash.label(issue.minimum))
        is FundingAmountIssue.AboveMaximum -> FundingSummaryProblem.Maximum(cash.label(issue.maximum))
        FundingAmountIssue.NotEnoughBalance, null -> FundingSummaryProblem.NoProvider
    }
}

fun etaOf(seconds: ULong): FundingEta {
    val minutes = max(1, ceil(seconds.toDouble() / SECONDS_PER_MINUTE).toInt())
    if (minutes < MINUTES_PER_HOUR) return FundingEta.Minutes(minutes)

    val hours = ceil(minutes.toDouble() / MINUTES_PER_HOUR).toInt()
    if (hours < HOURS_PER_DAY) return FundingEta.Hours(hours)

    return FundingEta.Days(ceil(hours.toDouble() / HOURS_PER_DAY).toInt())
}

/** Fees are counted in what the user pays: the ask's asset for value in, the balance for value out. */
fun FundingFlowState.toFeesUiState(): FundingFeesUiState? {
    val quote = selectedQuote ?: return null
    val inbound = direction == FundingDirection.IN
    val feeText = { units: String -> if (inbound) quoteUnit.format(units) else cash.label(cash.decimal(units)) }
    val total = (quote.providerFee.toBigIntegerOrNull() ?: BigInteger.ZERO) + (quote.networkFee.toBigIntegerOrNull() ?: BigInteger.ZERO)

    return FundingFeesUiState(
        payTitle = when {
            !inbound -> FundingPayTitle.YOU_SEND
            rail == FundingRail.CARD -> FundingPayTitle.YOU_PAY
            else -> FundingPayTitle.AMOUNT_TO_SEND
        },
        providerFee = feeText(quote.providerFee),
        networkFee = feeText(quote.networkFee),
        totalFee = feeText(total.toString()),
        pay = feeText(quote.sendAmount),
        rate = rateOf(quote),
    )
}

/** What one unit of the balance is worth in the provider-side asset. */
private fun FundingFlowState.rateOf(quote: FundingQuote): FundingRate? {
    val inbound = direction == FundingDirection.IN
    val providerSide = quoteUnit.decimal(if (inbound) quote.sendAmount else quote.receiveAmount)
    val cashSide = cash.decimal(if (inbound) quote.receiveAmount else quote.sendAmount)
    if (cashSide.signum() <= 0) return null

    val perCash = providerSide.divide(cashSide, RATE_SCALE, RoundingMode.HALF_EVEN)
    return FundingRate(symbol = cash.symbol, perCash = quoteUnit.format(perCash))
}

fun FundingFlowState.toCountryUiState(
    countries: List<FundingCountry>,
    detected: FundingCountry?,
    query: String,
): FundingCountryUiState {
    val refused = unsupportedCountries
    val matching = if (query.isBlank()) {
        countries
    } else {
        countries.filter { country ->
            country.name.contains(query, ignoreCase = true) ||
                country.currencyName?.contains(query, ignoreCase = true) == true ||
                country.currencyCode?.contains(query, ignoreCase = true) == true
        }
    }
    val showDetected = query.isBlank() && detected != null
    val toUi = { entry: FundingCountry -> entry.toUi(refused = entry.code in refused, selected = entry == country) }

    return FundingCountryUiState(
        query = query,
        detected = detected?.takeIf { showDetected }?.let(toUi),
        supported = matching
            .filter { it.code !in refused && !(showDetected && it == detected) }
            .map(toUi)
            .toImmutableList(),
        unsupported = matching.filter { it.code in refused }.map(toUi).toImmutableList(),
    )
}

private fun FundingCountry.toUi(refused: Boolean, selected: Boolean) = FundingCountryUi(
    code = code,
    flag = flag,
    name = name,
    currencyName = currencyName,
    refused = refused,
    selected = selected,
)

fun FundingFlowState.toProvidersUiState(
    brands: Map<String, FundingProviderBrand>,
    now: Instant,
): FundingProvidersUiState {
    val best = bestProviderId
    val selected = selectedProviderId
    val severalQuoted = quotedProviders.size > 1
    val expiry = quoteExpiry

    return FundingProvidersUiState(
        countdown = when {
            rows.values.any { it.isPending } -> FundingCountdown.Pending
            expiry != null -> FundingCountdown.Remaining(max(0L, (expiry - now).inWholeSeconds))
            else -> null
        },
        rows = servingCandidates.map { candidate ->
            val id = candidate.providerId
            val state = rows[id] ?: FundingQuoteState.Pending
            FundingProviderRow(
                brand = brands.brandOf(id),
                selected = id == selected,
                enabled = state.quote != null,
                dimmed = state.quote == null && !state.isPending,
                badge = providerBadge(state, isBest = id == best && severalQuoted),
                price = providerPrice(state),
            )
        }.toImmutableList(),
    )
}

private fun FundingFlowState.providerBadge(state: FundingQuoteState, isBest: Boolean): FundingProviderBadge? {
    if (state !is FundingQuoteState.Unavailable) return FundingProviderBadge.LowestPrice.takeIf { isBest }

    return when (val reason = state.reason) {
        FundingQuoteUnavailable.Timeout -> FundingProviderBadge.Unavailable
        is FundingQuoteUnavailable.Refused -> when (val refusal = reason.reason) {
            is FundingQuoteRefusal.BelowMinimum -> FundingProviderBadge.Minimum(cash.label(cash.decimal(refusal.min)))
            is FundingQuoteRefusal.AboveMaximum -> FundingProviderBadge.Maximum(cash.label(cash.decimal(refusal.max)))
            FundingQuoteRefusal.CountryUnsupported -> FundingProviderBadge.CountryUnsupported
            FundingQuoteRefusal.Unavailable, is FundingQuoteRefusal.Other -> FundingProviderBadge.Unavailable
        }
    }
}

private fun FundingFlowState.providerPrice(state: FundingQuoteState): FundingProviderPrice? = when (state) {
    FundingQuoteState.Pending -> FundingProviderPrice.Pending
    is FundingQuoteState.Quoted -> {
        val inbound = direction == FundingDirection.IN
        val quote = state.quote
        FundingProviderPrice.Quoted(
            price = quoteUnit.formatWithCode(if (inbound) quote.sendAmount else quote.receiveAmount),
            forAmount = cash.label(cash.decimal(if (inbound) quote.receiveAmount else quote.sendAmount)),
        )
    }

    is FundingQuoteState.Unavailable -> null
}

private fun Map<String, FundingProviderBrand>.brandOf(providerId: String) =
    get(providerId) ?: FundingProviderBrand(providerId = providerId, name = providerId, iconUrl = null)
