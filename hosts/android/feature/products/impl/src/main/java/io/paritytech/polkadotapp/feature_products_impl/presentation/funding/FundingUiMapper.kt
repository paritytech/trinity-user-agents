package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingReasonText
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FUNDING_RAIL_ORDER
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAmountIssue
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAssetUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingNetwork
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isPending
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDeposit
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
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
import io.paritytech.polkadotapp.common.R as RCommon

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
    null -> amountLimits.minimum?.let {
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
        headline = quote?.let { unitOf(it).format(if (inbound) it.sendAmount else it.receiveAmount) },
        isQuoting = isQuoting,
        showCountry = rail != FundingRail.CRYPTO,
        country = country?.toUi(refused = false, minimum = null, selected = true),
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
    start.failure != null -> FundingSummaryProblem.Failed(start.failure)
    start.failed -> FundingSummaryProblem.StartFailed
    !noProviderQuoted -> null
    else -> when (val issue = unavailableIssue) {
        is FundingAmountIssue.BelowMinimum -> FundingSummaryProblem.Minimum(cash.label(issue.minimum))
        is FundingAmountIssue.AboveMaximum -> FundingSummaryProblem.Maximum(cash.label(issue.maximum))
        FundingAmountIssue.NotEnoughBalance, null -> noProviderProblem()
    }
}

private fun FundingFlowState.noProviderProblem(): FundingSummaryProblem =
    quoteRefusedText()?.let(FundingSummaryProblem::QuoteRefused) ?: FundingSummaryProblem.NoProvider

/** Why no provider quoted, when they all said the same thing: no answer in time, the country, or a reason of their own. */
private fun FundingFlowState.quoteRefusedText(): FundingReasonText? {
    val reasons = rows.values.map { state ->
        when (val reason = (state as? FundingQuoteState.Unavailable)?.reason) {
            FundingQuoteUnavailable.Timeout -> FundingReasonText.Res(RCommon.string.funding_error_quote_timeout)
            is FundingQuoteUnavailable.Refused -> when (val refusal = reason.reason) {
                FundingQuoteRefusal.CountryUnsupported -> FundingReasonText.Res(RCommon.string.funding_providers_country_unsupported)
                is FundingQuoteRefusal.Other -> refusal.message.takeIf { it.isNotBlank() }?.let(FundingReasonText::Verbatim)
                else -> null
            }

            null -> null
        }
    }
    val first = reasons.firstOrNull() ?: return null
    return first.takeIf { reasons.all { it == first } }
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
    val quoteUnit = unitOf(quote)
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
    val quoteUnit = unitOf(quote)
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
    val minimums = countryMinimums.filterValues { it > amount }
    val showDetected = query.isBlank() && detected != null
    val toUi = { entry: FundingCountry ->
        entry.toUi(refused = entry.code in refused, minimum = minimums[entry.code]?.let(cash::label), selected = entry == country)
    }
    val listed = matching.filter { !(showDetected && it == detected) }

    return FundingCountryUiState(
        query = query,
        detected = detected?.takeIf { showDetected }?.let(toUi),
        supported = listed.filter { it.code !in refused && it.code !in minimums }.map(toUi).toImmutableList(),
        belowMinimum = listed.filter { it.code !in refused && it.code in minimums }.map(toUi).toImmutableList(),
        unsupported = listed.filter { it.code in refused }.map(toUi).toImmutableList(),
    )
}

private fun FundingCountry.toUi(refused: Boolean, minimum: String?, selected: Boolean) = FundingCountryUi(
    code = code,
    flag = flag,
    name = name,
    currencyName = currencyName,
    refused = refused,
    minimum = minimum,
    selected = selected,
)

/** Counts down to the core's next refresh of the prices on screen; while it runs, the countdown holds at zero. */
fun FundingFlowState.toProvidersUiState(
    brands: Map<String, FundingProviderBrand>,
): FundingProvidersUiState {
    val selected = selectedProviderId
    val severalQuoted = quotedProviders.size > 1
    val pending = rows.values.any { it.isPending }

    return FundingProvidersUiState(
        countdown = when {
            pending && staleQuotes.isNotEmpty() -> FundingCountdown.Refreshing
            pending -> FundingCountdown.Pending
            refreshAt != null -> FundingCountdown.Until(refreshAt)
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
                price = providerPrice(state, staleQuotes[id]),
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

/** A provider asked again keeps what its last price was for, with the price itself loading. */
private fun FundingFlowState.providerPrice(state: FundingQuoteState, stale: FundingQuote?): FundingProviderPrice? = when (state) {
    FundingQuoteState.Pending -> stale?.let { FundingProviderPrice.Updating(forAmount = forAmount(it)) } ?: FundingProviderPrice.Pending
    is FundingQuoteState.Quoted -> {
        val inbound = direction == FundingDirection.IN
        val quote = state.quote
        FundingProviderPrice.Quoted(
            price = unitOf(quote).formatWithCode(if (inbound) quote.sendAmount else quote.receiveAmount),
            forAmount = forAmount(quote),
        )
    }

    is FundingQuoteState.Unavailable -> null
}

private fun FundingFlowState.forAmount(quote: FundingQuote): String =
    cash.label(cash.decimal(if (direction == FundingDirection.IN) quote.receiveAmount else quote.sendAmount))

private fun Map<String, FundingProviderBrand>.brandOf(providerId: String) =
    get(providerId) ?: FundingProviderBrand(providerId = providerId, name = providerId, iconUrl = null)

private const val SHORT_ADDRESS_MIN_LENGTH = 14
private const val SHORT_ADDRESS_HEAD = 6
private const val SHORT_ADDRESS_TAIL = 5

/** A network stays closed to amounts below the lowest minimum a provider has refused with there. */
fun FundingFlowState.toNetworkRows(): ImmutableList<FundingNetworkRow> = networks.map { network ->
    FundingNetworkRow(
        id = network.id,
        name = network.name,
        monogram = network.monogram,
        minimum = minimum(network.id)?.takeIf { it > amount }?.let(cash::label),
    )
}.toImmutableList()

fun FundingFlowState.toTokensUiState(): FundingTokensUiState {
    val selected = network?.let(::FundingNetwork)
    return FundingTokensUiState(
        networkName = selected?.name,
        networkMonogram = selected?.monogram,
        rows = tokens.map { FundingTokenRow(it.symbol, it.monogram) }.toImmutableList(),
    )
}

fun FundingFlowState.toDepositUiState(progress: FundingProgress?, start: FundingStartState): FundingDepositUiState {
    val content = when (val deposit = progress?.deposit) {
        is FundingDeposit.Crypto -> FundingDepositContent.Crypto(
            qrPayload = deposit.uri ?: deposit.address,
            amount = FundingAssetUnit.of(deposit.asset, deposit.decimals.toInt()).format(deposit.amount),
            exact = deposit.exact,
            address = deposit.address,
            shortAddress = shortened(deposit.address),
            networkName = FundingNetwork(deposit.network).name,
        )

        is FundingDeposit.Bank -> FundingDepositContent.Bank(
            amount = FundingAssetUnit.of(deposit.currency, deposit.decimals.toInt()).format(deposit.amount),
            beneficiary = deposit.beneficiary,
            account = deposit.account,
            bankCode = deposit.bankCode,
            reference = deposit.reference,
        )

        null -> when {
            start.failed -> FundingDepositContent.Problem(FundingSummaryProblem.StartFailed)
            noProviderQuoted && !start.started -> FundingDepositContent.Problem(noProviderProblem())
            else -> FundingDepositContent.Waiting(preparing = start.started, card = rail == FundingRail.CARD)
        }
    }

    return FundingDepositUiState(
        rail = rail,
        started = start.started,
        failed = start.failure != null,
        isCancelling = start.isCancelling,
        content = start.failure?.let { FundingDepositContent.Problem(FundingSummaryProblem.Failed(it)) } ?: content,
    )
}

private fun shortened(address: String): String =
    if (address.length <= SHORT_ADDRESS_MIN_LENGTH) address else "${address.take(SHORT_ADDRESS_HEAD)}…${address.takeLast(SHORT_ADDRESS_TAIL)}"
