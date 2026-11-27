package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingCancelAvailability
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import uniffi.truapi.U128

/** The core's funding state as plain fields, and every call made to it in order. */
class FakeFundingCore : FundingCore {
    val sessions = mutableMapOf<String, FundingSession>()
    val progress = mutableMapOf<String, FundingProgress>()
    val options = mutableMapOf<String, FundingOptions>()
    val quotes = mutableMapOf<String, FundingQuoteList>()
    val summaries = mutableMapOf<String, FundingSessionSummary>()
    val history = mutableListOf<FundingSessionSummary>()
    val calls = mutableListOf<String>()
    var nextRequestId = 1uL
    var providerTakes = true
    var quoteRequestFails = false
    var quotesReadFails = false
    var autoStartFails = false
    var cancelOutcome = CancelOutcome.REQUESTED
    val waitingFrames = mutableSetOf<String>()

    override suspend fun start(): Result<Unit> = Result.success(Unit).also { calls += "start" }

    override suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?> =
        Result.success<FundingIntentId?>(FundingIntentId("opened")).also { calls += "open $direction ${amount?.value}" }

    override suspend fun session(intentId: FundingIntentId): Result<FundingSession?> = Result.success(sessions[intentId.value])

    override suspend fun progress(intentId: FundingIntentId): Result<FundingProgress?> = Result.success(progress[intentId.value])

    override suspend fun options(intentId: FundingIntentId): Result<FundingOptions?> = Result.success(options[intentId.value])

    override suspend fun quotes(intentId: FundingIntentId): Result<FundingQuoteList?> =
        if (quotesReadFails) Result.failure(IllegalStateException("quotes read")) else Result.success(quotes[intentId.value])

    override suspend fun summary(intentId: FundingIntentId): Result<FundingSessionSummary?> = Result.success(summaries[intentId.value])

    override suspend fun entries(): Result<List<FundingBookEntry>> =
        Result.success(sessions.values.map { FundingBookEntry(it, progress[it.intent] ?: progress()) })

    override suspend fun history(): Result<List<FundingSessionSummary>> = Result.success(history.toList())

    override suspend fun select(intentId: FundingIntentId, selection: FundingSelection): Result<Unit> =
        Result.success(Unit).also { calls += "select ${selection.rail}" }

    override suspend fun requestQuote(intentId: FundingIntentId, amount: Balance): Result<ULong?> {
        calls += "quote ${amount.value}"
        return if (quoteRequestFails) Result.failure(IllegalStateException("quote request")) else Result.success(nextRequestId++)
    }

    override suspend fun startWhenQuoted(intentId: FundingIntentId, requestId: ULong): Result<Unit> {
        calls += "startWhenQuoted $requestId"
        return if (autoStartFails) Result.failure(IllegalStateException("auto-start")) else Result.success(Unit)
    }

    override suspend fun amountIssue(intentId: FundingIntentId, amount: Balance, spendable: Balance?): Result<FundingAmountIssue?> =
        Result.success(null)

    override suspend fun selectProvider(intentId: FundingIntentId, providerId: ProductId, quoteId: String): Result<Boolean> {
        calls += "selectProvider ${providerId.value} $quoteId"
        if (providerTakes) sessions[intentId.value]?.let { sessions[intentId.value] = it.copy(providerId = providerId) }
        return Result.success(providerTakes)
    }

    override suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome> =
        Result.success(cancelOutcome).also { calls += "cancel" }

    override suspend fun confirmCancel(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation> =
        Result.success(CancelConfirmation.ACCEPTED).also { calls += "confirmCancel $terms" }

    override suspend fun acknowledge(intentId: FundingIntentId): Result<Boolean> =
        Result.success(true).also { calls += "acknowledge ${intentId.value}" }

    override suspend fun openFrame(intentId: FundingIntentId): Result<Boolean> =
        Result.success(waitingFrames.remove(intentId.value)).also { calls += "openFrame ${intentId.value}" }

    override suspend fun frameWaiting(intentId: FundingIntentId): Result<Boolean> = Result.success(intentId.value in waitingFrames)

    companion object {
        fun session(
            intent: String,
            providerId: ProductId? = null,
            stage: FundingStage = FundingStage.Open,
            openedAtMs: ULong = 1uL,
        ) = FundingSession(
            intent = intent,
            ownerProductId = null,
            direction = FundingDirection.IN,
            amount = null,
            stage = stage,
            openedAtMs = openedAtMs,
            deadlineMs = 0uL,
            acknowledged = false,
            providerId = providerId,
            assignedAtMs = null,
            cancel = null,
            updates = emptyList(),
            started = emptyList(),
            choice = null,
            refund = null,
            detail = null,
        )

        fun progress() = FundingProgress(
            steps = emptyList(),
            band = FundingBand.AWAITING_PAYMENT,
            lastHeardAtMs = null,
            detail = null,
            failedAtMs = null,
            transactionId = null,
            reference = null,
            deposit = null,
            mismatch = null,
            retrying = false,
            payout = null,
            refunded = null,
            status = FundingStatus.WaitingForPayment,
            awaitsUserTransfer = false,
            longerAtMs = null,
            moneyMoved = false,
            cancel = FundingCancelAvailability.UNAVAILABLE,
        )

        fun options(vararg providers: ProductId) = FundingOptions(
            candidates = providers.map {
                FundingCandidate(it, routes = emptyList(), unsupported = emptyList(), limits = emptyList(), declaresRoutes = false, brand = null)
            },
            countries = emptyList(),
            limits = FundingAmountLimits(min = null, max = null),
            networks = emptyList(),
            rails = listOf(FundingRail.CARD),
            asset = "EUR",
            tokens = emptyList(),
            detectedCountry = null,
        )

        fun quote(id: String) = FundingQuote(
            quoteId = id,
            sendAmount = "100",
            receiveAmount = "90",
            providerFee = "5",
            networkFee = "5",
            etaSecs = null,
            expiresAt = null,
            assetDecimals = 2u,
        )

        fun list(requestId: ULong, amount: U128, vararg rows: Pair<ProductId, FundingQuoteState>, complete: Boolean = true) =
            FundingQuoteList(
                requestId = requestId,
                rows = rows.map { (provider, state) -> FundingQuoteRow(provider, state) },
                best = rows.firstOrNull { it.second is FundingQuoteState.Quoted }?.first,
                refreshAtMs = 0uL,
                amount = amount,
                refusal = null,
                issue = null,
                complete = complete,
            )
    }
}
