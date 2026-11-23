package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.orZero
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.FundingReasonText
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isPending
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingCancelAvailability
import uniffi.truapi.FundingDeposit
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStep
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import uniffi.truapi.fundingNetworkName
import java.math.BigDecimal
import java.math.RoundingMode
import kotlin.math.ceil
import kotlin.math.max
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

private val PRESETS = listOf(BigDecimal("10"), BigDecimal("50"), BigDecimal("100"))

fun FundingFlowState.toAmountUiState(history: FundingHistoryClock): FundingAmountUiState {
    val withdraw = direction == FundingDirection.OUT
    val available = availableRails

    return FundingAmountUiState(
        isWithdraw = withdraw,
        rails = FUNDING_RAIL_ORDER.map { FundingRailTab(it, selected = it == rail, enabled = it in available) }.toImmutableList(),
        amount = format.figure(amount),
        hasAmount = amount.signum() > 0,
        available = spendable?.takeIf { withdraw }?.let { format.label(cashAmount(it)) },
        note = amountNote(),
        presets = if (withdraw) persistentListOf() else PRESETS.map { FundingPreset(it, format.figure(it)) }.toImmutableList(),
        canContinue = canContinue,
        history = history,
    )
}

private fun FundingFlowState.amountNote(): FundingAmountNote? = when (val issue = amountIssue) {
    is FundingAmountIssue.BelowMinimum -> FundingAmountNote.BelowMinimum(cashLabel(issue.min))
    is FundingAmountIssue.AboveMaximum -> FundingAmountNote.AboveMaximum(cashLabel(issue.max))
    FundingAmountIssue.NotEnoughBalance -> FundingAmountNote.NotEnough(format.symbol())
    null -> amountLimits.minimum?.let {
        FundingAmountNote.Minimum(format.label(it), withdraw = direction == FundingDirection.OUT)
    }
}

private const val SECONDS_PER_MINUTE = 60.0
private const val MINUTES_PER_HOUR = 60
private const val HOURS_PER_DAY = 24
private const val RATE_SCALE = 4

fun FundingFlowState.toSummaryUiState(
    brands: Map<ProductId, FundingProviderBrand>,
    start: FundingStartState,
    failure: FundingFailureReason?,
): FundingSummaryUiState {
    val quote = selectedQuote
    val inbound = direction == FundingDirection.IN

    return FundingSummaryUiState(
        isWithdraw = !inbound,
        rail = rail,
        headline = quote?.let { if (inbound) unitOf(it).format(it.sendAmount.balance()) else cashLabel(it.sendAmount) },
        isQuoting = isQuoting,
        showCountry = rail != FundingRail.CRYPTO,
        country = country?.toUi(refused = false, minimum = null, selected = true),
        provider = selectedProviderId?.let { brands.brandOf(it) },
        payout = quote?.let { if (inbound) cashLabel(it.receiveAmount) else unitOf(it).format(it.receiveAmount.balance()) },
        eta = quote?.etaSecs?.let(::etaOf),
        bankRateNoteSymbol = format.symbol().takeIf { rail == FundingRail.BANK && inbound },
        problem = summaryProblem(start, failure),
        canStart = quote != null && (start == FundingStartState.Idle || start == FundingStartState.StartFailed),
        isStarting = start == FundingStartState.Starting,
    )
}

private fun FundingFlowState.summaryProblem(start: FundingStartState, failure: FundingFailureReason?): FundingSummaryProblem? = when {
    failure != null -> FundingSummaryProblem.Failed(failure)
    start == FundingStartState.StartFailed -> FundingSummaryProblem.StartFailed
    !noProviderQuoted -> null
    else -> when (val issue = unavailableIssue) {
        is FundingAmountIssue.BelowMinimum -> FundingSummaryProblem.Minimum(cashLabel(issue.min))
        is FundingAmountIssue.AboveMaximum -> FundingSummaryProblem.Maximum(cashLabel(issue.max))
        FundingAmountIssue.NotEnoughBalance, null -> noProviderProblem()
    }
}

private fun FundingFlowState.noProviderProblem(): FundingSummaryProblem =
    quoteRefusedText()?.let(FundingSummaryProblem::QuoteRefused) ?: FundingSummaryProblem.NoProvider

/** Why no provider quoted, when the core says they all gave the same reason: no answer in time, the country, or their own. */
private fun FundingFlowState.quoteRefusedText(): FundingReasonText? = when (val refusal = list?.refusal) {
    FundingQuoteUnavailable.Timeout -> FundingReasonText.Res(RCommon.string.funding_error_quote_timeout)
    is FundingQuoteUnavailable.Refused -> when (val reason = refusal.reason) {
        FundingQuoteRefusal.CountryUnsupported -> FundingReasonText.Res(RCommon.string.funding_providers_country_unsupported)
        is FundingQuoteRefusal.Other -> reason.message.takeIf { it.isNotBlank() }?.let(FundingReasonText::Verbatim)
        else -> null
    }
    null -> null
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
    val feeText = { units: Balance -> if (inbound) quoteUnit.format(units) else format.cost(cashAmount(units)) }
    val total = quote.totalFee()

    return FundingFeesUiState(
        payTitle = when {
            !inbound -> FundingPayTitle.YOU_SEND
            rail == FundingRail.CARD -> FundingPayTitle.YOU_PAY
            else -> FundingPayTitle.AMOUNT_TO_SEND
        },
        amount = feeText(quote.netOfFees()),
        providerFee = feeText(quote.providerFee.balance()),
        networkFee = feeText(quote.networkFee.balance()),
        totalFee = feeText(total),
        pay = if (inbound) quoteUnit.format(quote.sendAmount.balance()) else cashLabel(quote.sendAmount),
        rate = rateOf(quote),
    )
}

private fun FundingQuote.totalFee(): Balance = providerFee.balance() + networkFee.balance()

/** What the user pays less the fees, in the unit of `sendAmount`. */
private fun FundingQuote.netOfFees(): Balance = (sendAmount.balance() - totalFee()).atLeastZero()

/** What one unit of the balance is worth in the provider-side asset, net of fees as the amount line. */
private fun FundingFlowState.rateOf(quote: FundingQuote): FundingRate? {
    val inbound = direction == FundingDirection.IN
    val quoteUnit = unitOf(quote)
    val net = quote.netOfFees()
    val providerSide = quoteUnit.decimal(if (inbound) net else quote.receiveAmount.balance())
    val cashSide = cashAmount(if (inbound) quote.receiveAmount.balance() else net)
    if (cashSide.signum() <= 0) return null

    val perCash = providerSide.divide(cashSide, RATE_SCALE, RoundingMode.HALF_EVEN)
    return FundingRate(symbol = format.symbol(), perCash = quoteUnit.format(perCash))
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
        entry.toUi(refused = entry.code in refused, minimum = minimums[entry.code]?.let(format::label), selected = entry == country)
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
    brands: Map<ProductId, FundingProviderBrand>,
    now: Instant,
): FundingProvidersUiState {
    val selected = selectedProviderId
    val severalQuoted = quotedProviders.size > 1
    val pending = rows.values.any { it.isPending }
    val refreshAt = refreshAt

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
        now = now,
    )
}

private fun FundingFlowState.providerBadge(state: FundingQuoteState, isBest: Boolean): FundingProviderBadge? {
    if (state !is FundingQuoteState.Unavailable) return FundingProviderBadge.LowestPrice.takeIf { isBest }

    return when (val reason = state.reason) {
        FundingQuoteUnavailable.Timeout -> FundingProviderBadge.Unavailable
        is FundingQuoteUnavailable.Refused -> when (val refusal = reason.reason) {
            is FundingQuoteRefusal.BelowMinimum -> FundingProviderBadge.Minimum(cashLabel(refusal.min))
            is FundingQuoteRefusal.AboveMaximum -> FundingProviderBadge.Maximum(cashLabel(refusal.max))
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
            price = unitOf(quote).formatWithCode((if (inbound) quote.sendAmount else quote.receiveAmount).balance()),
            forAmount = forAmount(quote),
        )
    }

    is FundingQuoteState.Unavailable -> null
}

private fun FundingFlowState.forAmount(quote: FundingQuote): String =
    cashLabel(if (direction == FundingDirection.IN) quote.receiveAmount else quote.sendAmount)

private fun Map<ProductId, FundingProviderBrand>.brandOf(providerId: ProductId) =
    get(providerId) ?: FundingProviderBrand(providerId = providerId, name = providerId.value, iconUrl = null)

/** A network stays closed to amounts below the least the core says it takes. */
fun FundingFlowState.toNetworkRows(): ImmutableList<FundingNetworkRow> = networks.map { id ->
    FundingNetworkRow(
        id = id,
        name = fundingNetworkName(id),
        icon = fundingNetworkIcon(id, networkIconProvider(id)),
        minimum = minimum(id)?.takeIf { it > amount }?.let(format::label),
    )
}.toImmutableList()

fun FundingFlowState.toTokensUiState(): FundingTokensUiState {
    return FundingTokensUiState(
        networkName = network?.let(::fundingNetworkName),
        networkIcon = network?.let { fundingNetworkIcon(it, networkIconProvider(it)) },
        rows = tokens.map { FundingTokenRow(it.symbol, fundingTokenIcon(it.symbol, tokenIconProvider(it.symbol))) }.toImmutableList(),
    )
}

fun FundingFlowState.toDepositUiState(
    progress: FundingProgress?,
    session: FundingSessionFacts,
    isCancelling: Boolean,
    now: Instant,
): FundingDepositUiState {
    val content = when (val deposit = progress?.deposit) {
        // Only the app's own network reports an address; a provider serving another one takes the payment in its own
        // screen. Once the provider has the funds the address is no longer somewhere to send to.
        is FundingDeposit.Crypto -> when {
            progress?.hasReceived() == true -> FundingDepositContent.ProgressOnly
            else -> FundingDepositContent.Crypto(
                qrPayload = deposit.uri ?: deposit.address,
                amount = FundingAssetUnit.of(deposit.asset, deposit.decimals.toInt()).format(deposit.amount.balance()),
                exact = deposit.exact,
                address = deposit.address,
                networkName = fundingNetworkName(deposit.network),
                networkIcon = fundingNetworkIcon(deposit.network, session.providerId ?: selectedProviderId),
            )
        }

        is FundingDeposit.Bank -> FundingDepositContent.Bank(
            amount = FundingAssetUnit.of(deposit.currency, deposit.decimals.toInt()).format(deposit.amount.balance()),
            beneficiary = deposit.beneficiary,
            account = deposit.account,
            bankCode = deposit.bankCode,
            reference = deposit.reference,
        )

        null -> when {
            noProviderQuoted && !session.started -> FundingDepositContent.Problem(noProviderProblem())
            session.started && rail == FundingRail.CRYPTO -> FundingDepositContent.ProgressOnly
            else -> FundingDepositContent.Waiting(preparing = session.started, card = rail == FundingRail.CARD)
        }
    }

    return FundingDepositUiState(
        rail = rail,
        withdraw = direction == FundingDirection.OUT,
        started = session.started,
        failed = session.failure != null,
        isCancelling = isCancelling,
        cancel = session.cancel?.let(::cancelUi),
        moneyMoved = progress?.moneyMoved == true,
        cancelAvailability = progress?.cancel ?: FundingCancelAvailability.UNAVAILABLE,
        figure = format.figure(amount),
        progress = progress?.takeIf { session.started }?.let { live ->
            FundingLiveProgressUi(
                timeline = FundingTimeline.live(live.steps, live.band, direction, failed = session.failure != null),
                band = bandLine(live.band, (live.deposit as? FundingDeposit.Crypto)?.asset ?: asset),
                lastHeardAt = live.lastHeardAtMs?.let { Instant.fromEpochMilliseconds(it.toLong()) },
                detail = live.detail,
            )
        },
        content = session.failure?.let { FundingDepositContent.Problem(FundingSummaryProblem.Failed(it)) } ?: content,
        now = now,
    )
}

/** The provider has the user's payment: the received milestone, or one past it. */
private fun FundingProgress.hasReceived(): Boolean = steps.any { it.reachedAtMs != null && it.step != FundingStep.STARTED }

/** The band line, worded per `FundingBand`'s docs for this session's rail and [asset]. */
fun FundingFlowState.bandLine(band: FundingBand, asset: String?): FundingBandUi? {
    val crypto = rail == FundingRail.CRYPTO
    val inbound = direction == FundingDirection.IN
    val coin = asset.orEmpty()
    fun line(text: Int, arg: String? = null) = FundingBandUi(text, arg, warning = false)
    fun warning(text: Int) = FundingBandUi(text, null, warning = true)
    return when (band) {
        FundingBand.AWAITING_PAYMENT -> when (rail) {
            FundingRail.CARD -> line(RCommon.string.funding_band_waiting_payment)
            FundingRail.BANK -> line(RCommon.string.funding_band_waiting_bank)
            FundingRail.CRYPTO -> line(RCommon.string.funding_band_waiting_deposit)
        }
        FundingBand.CHECKING_PAYMENT ->
            if (crypto) line(RCommon.string.funding_band_confirming_deposit, coin) else line(RCommon.string.funding_band_checking)
        FundingBand.ON_ITS_WAY -> line(RCommon.string.funding_band_on_its_way)
        FundingBand.ADDING -> line(RCommon.string.funding_band_adding, format.label(amount))
        FundingBand.ADDED -> line(RCommon.string.funding_band_added, format.label(amount))
        FundingBand.CONVERTING -> line(RCommon.string.funding_band_converting)
        FundingBand.PREPARING_PAYOUT ->
            if (crypto) line(RCommon.string.funding_band_exchanging, coin) else line(RCommon.string.funding_band_preparing_payout)
        FundingBand.SENDING -> when (rail) {
            FundingRail.CARD -> line(RCommon.string.funding_band_sending_card)
            FundingRail.BANK -> line(RCommon.string.funding_band_sending_bank)
            FundingRail.CRYPTO -> line(RCommon.string.funding_band_sending_asset, coin)
        }
        FundingBand.SENT -> when (rail) {
            FundingRail.CARD -> line(RCommon.string.funding_band_sent_card)
            FundingRail.BANK -> line(RCommon.string.funding_band_sent_bank)
            FundingRail.CRYPTO -> line(RCommon.string.funding_band_sent_asset, coin)
        }
        FundingBand.RETRYING -> warning(
            when {
                !inbound -> RCommon.string.funding_band_retrying_payout
                rail == FundingRail.BANK -> RCommon.string.funding_band_retrying_bank
                else -> RCommon.string.funding_band_retrying
            },
        )
        FundingBand.DELAYED -> warning(if (inbound) RCommon.string.funding_band_delayed else RCommon.string.funding_band_delayed_out)
        FundingBand.CANCEL, FundingBand.ENDED -> null
    }
}

private fun FundingFlowState.cancelUi(cancel: FundingCancel): FundingCancelUiState? = when (cancel) {
    FundingCancel.Requested -> FundingCancelUiState.Requested
    is FundingCancel.Refused -> FundingCancelUiState.Refused(cancel.reason)
    is FundingCancel.Offered -> FundingCancelUiState.Offered(
        fee = format.cost(cashAmount(cancel.fee.balance())),
        refund = cashLabel(cancel.refund),
        expiresAt = cancel.expiresAt?.let { Instant.fromEpochMilliseconds(it.toLong()) },
    )
    is FundingCancel.Accepted -> FundingCancelUiState.Cancelling
    FundingCancel.Declined, is FundingCancel.Overtaken -> null
}

private fun FundingFlowState.cashLabel(units: U128): String = format.label(cashAmount(units.balance()))

/** A core amount as a balance; zero when it is not a number. */
private fun U128.balance(): Balance = toBalanceOrNull().orZero()
