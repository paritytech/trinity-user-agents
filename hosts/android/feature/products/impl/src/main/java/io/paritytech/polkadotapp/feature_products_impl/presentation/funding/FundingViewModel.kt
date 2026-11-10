package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.asLoaded
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountries
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayMode
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.applying
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.failure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isOpen
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.updateAndGet
import kotlinx.coroutines.launch
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import java.math.BigDecimal
import kotlin.time.Duration.Companion.milliseconds
import io.paritytech.polkadotapp.common.R as RCommon
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Built for the overlay [context] its sheet found when it opened, which the overlay may have dropped since. */
@HiltViewModel(assistedFactory = FundingViewModel.Factory::class)
class FundingViewModel @AssistedInject constructor(
    @Assisted private val context: FundingOverlayContext,
    private val contexts: FundingOverlayContexts,
    private val interactor: FundingFlowInteractor,
    private val router: ProductsRouter,
    private val clipboard: ClipboardService,
) : BaseViewModel() {
    private companion object {
        val REQUOTE_DELAY = 600.milliseconds
    }

    private val intent = context.request.intent
    private val flow = MutableStateFlow<FundingFlowState?>(null)
    private val path = MutableStateFlow<List<FundingScreen>>(emptyList())
    private val start = MutableStateFlow(
        FundingStartState(
            isStarting = false,
            failed = false,
            started = false,
            failure = null,
            isCancelling = false,
            cancel = null,
        ),
    )
    private val progress = MutableStateFlow<FundingProgress?>(null)
    private var quoteJob: Job? = null

    /** The ask a crypto top-up starts on once every provider answered it, while its deposit screen is on top. */
    private var startsWhenQuoted: FundingQuoteAsk? = null

    private val brands = MutableStateFlow<Map<ProductId, FundingProviderBrand>>(emptyMap())
    private val brandsLoading = mutableSetOf<ProductId>()
    private val countryQuery = MutableStateFlow("")
    private val countries by lazy { interactor.countries() }
    private val detectedCountry = MutableStateFlow<FundingCountry?>(null)

    val state: StateFlow<LoadingState<FundingSheetUiState>> = combine(
        flow.filterNotNull(),
        path,
        combine(start, progress) { start, progress -> start to progress },
        brands,
        combine(countryQuery, detectedCountry) { query, detected -> query to detected },
    ) { flow, path, (start, progress), brands, (query, detected) ->
        FundingSheetUiState(
            screen = path.lastOrNull() ?: FundingScreen.AMOUNT,
            amount = flow.toAmountUiState(),
            summary = flow.toSummaryUiState(brands, start),
            fees = flow.toFeesUiState(),
            country = flow.toCountryUiState(countries, detected, query),
            providers = flow.toProvidersUiState(brands),
            networks = flow.toNetworkRows(),
            tokens = flow.toTokensUiState(),
            deposit = flow.toDepositUiState(progress, start),
        ).asLoaded()
    }.stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    init {
        launch { interactor.quoteLists(intent).collect { onQuoteList(it) } }
        launchUnit {
            val initial = initialState()
            flow.value = initial
            if (context.mode == FundingOverlayMode.Resumed) resume()
            launch { followSession() }
            launch { interactor.options(intent).filterNotNull().collect(::onOptions) }
            reportSelection()
        }
    }

    fun onNetworkChosen(id: String) {
        flow.update { it?.withNetwork(id) }
        push(FundingScreen.TOKEN)
    }

    fun onOpenNetworks() = push(FundingScreen.NETWORK)

    /** Value in goes straight to the deposit screen, which starts with the best quote once every provider answered. */
    fun onTokenChosen(symbol: String) {
        val current = flow.value ?: return
        flow.value = current.withAsset(symbol)
        requestQuote()

        if (current.direction == CoreFundingDirection.IN) {
            startsWhenQuoted = flow.value?.ask
            push(FundingScreen.DEPOSIT)
        } else {
            push(FundingScreen.SUMMARY)
        }
    }

    fun onCopy(value: String) {
        clipboard.setPrimaryClip(value)
        showMessage(RCommon.string.funding_deposit_copied)
    }

    fun onCancelTopUp() = push(FundingScreen.CANCEL_CONFIRM)

    /**
     * Asks to cancel a started session. Before money moves that ends it; after, it is a request the provider answers
     * with a refusal or an offer, which the deposit screen then shows.
     */
    fun onConfirmCancel() = launchUnit {
        if (start.value.isCancelling) return@launchUnit
        start.update { it.copy(isCancelling = true) }

        interactor.cancel(intent)
            .logFailure("Funding cancel failed")
            .onSuccess { outcome ->
                when (outcome) {
                    CancelOutcome.ENDED -> onClose()
                    CancelOutcome.REQUESTED, CancelOutcome.PENDING -> leaveCancelConfirm(RCommon.string.funding_cancel_requested)
                    CancelOutcome.TOO_LATE -> leaveCancelConfirm(RCommon.string.funding_cancel_too_late)
                    CancelOutcome.REFUSED -> leaveCancelConfirm(RCommon.string.funding_error_cancel_refused)
                }
            }
            .onFailure { leaveCancelConfirm(RCommon.string.funding_error_cancel_failed) }

        start.update { it.copy(isCancelling = false) }
    }

    /** Shows the provider's cancel offer again, after the user went back from it. */
    fun onReviewCancelOffer() {
        if (start.value.cancel is FundingCancel.Offered) push(FundingScreen.CANCEL_OFFER)
    }

    fun onAcceptCancelOffer() = answerCancelOffer(accept = true)

    fun onDeclineCancelOffer() = answerCancelOffer(accept = false)

    fun onRailSelected(rail: FundingRail) {
        flow.update { it?.withRail(rail) }
        scheduleQuote()
    }

    fun onKey(key: FundingKey) {
        flow.update { it?.withAmountText(it.amountText.applying(key)) }
        scheduleQuote()
    }

    fun onPreset(value: BigDecimal) {
        flow.update { it?.withAmountText(FundingFlowState.textFor(value)) }
        scheduleQuote()
    }

    fun onContinueFromAmount() {
        val current = flow.value?.takeIf { it.canContinue } ?: return

        when (current.rail) {
            FundingRail.CRYPTO -> onOpenNetworks()
            FundingRail.CARD, FundingRail.BANK -> {
                if (current.ask != current.currentAsk()) requestQuote()
                push(FundingScreen.SUMMARY)
            }
        }
    }

    fun onOpenFees() {
        if (flow.value?.selectedQuote != null) push(FundingScreen.FEES)
    }

    fun onOpenCountry() {
        countryQuery.value = ""
        push(FundingScreen.COUNTRY)
    }

    fun onOpenProviders() = push(FundingScreen.PROVIDERS)

    fun onCountryQueryChanged(query: String) {
        countryQuery.value = query
    }

    fun onCountryChosen(code: String) {
        val chosen = countries.firstOrNull { it.code == code } ?: return
        flow.update { it?.withCountry(chosen) }
        pop()
        requestQuote()
    }

    fun onProviderChosen(providerId: ProductId) {
        flow.update { it?.withChosenProvider(providerId) }
        showMessage(RCommon.string.funding_provider_changed)
        pop()
    }

    /** Hands the session to the provider whose quote is on screen, then answers `Started`. A provider is only chosen on its quote. */
    fun onStart() {
        val current = flow.value ?: return
        val providerId = current.selectedProviderId ?: return
        val quoteId = current.rows[providerId]?.quote?.quoteId ?: return
        if (start.value.isStarting || start.value.started) return

        start.update { it.copy(isStarting = true, failed = false) }
        launchUnit {
            val selected = interactor.selectProvider(intent, providerId, quoteId)
                .logFailure("Funding provider selection failed")
                .getOrDefault(false)

            start.update { it.copy(isStarting = false, failed = !selected, started = selected) }
            if (selected) didStart(current.rail)
        }
    }

    fun onBack() {
        val leavesStartedDeposit = start.value.started && path.value.lastOrNull() == FundingScreen.DEPOSIT
        if (path.value.isEmpty() || leavesStartedDeposit) onClose() else pop()
    }

    /** Before `Started` leaving is the answer the core discards the session on; after it the session runs on. */
    fun onClose() {
        quoteJob?.cancel()
        context.answer(FundingOverlayOutcome.DISMISSED)
        closeSheet()
    }

    override fun onCleared() {
        super.onCleared()
        context.answer(FundingOverlayOutcome.DISMISSED)
        // A sheet opened for the same session since then has put its own context in; that one stays.
        contexts.remove(context)
    }

    private fun closeSheet() = launchUnit {
        context.close()
        router.closeFundingOverlay(intent)
    }

    private suspend fun initialState(): FundingFlowState {
        val cash = interactor.cash()
        val request = context.request
        val options = interactor.options(intent).first()
        val detected = options?.detectedCountry?.let(FundingCountries::country)
        detectedCountry.value = detected

        return FundingFlowState.initial(
            direction = request.direction.toCore(),
            cash = cash,
            spendable = interactor.spendable(cash),
            options = options,
            amount = request.amount?.let { cash.decimal(it.value.toString()) },
            country = detected,
        )
    }

    private fun push(screen: FundingScreen) {
        path.update { it + screen }
    }

    private fun pop() {
        if (path.value.lastOrNull() == FundingScreen.DEPOSIT) startsWhenQuoted = null
        path.update { it.dropLast(1) }
    }

    private fun onQuoteList(list: FundingQuoteList) {
        val current = flow.value ?: return
        val updated = current.receiving(list)
        if (updated === current) return
        flow.value = updated

        val armed = startsWhenQuoted
        if (armed != null && updated.ask == armed && updated.allAnswered && path.value.lastOrNull() == FundingScreen.DEPOSIT) {
            startsWhenQuoted = null
            onStart()
        }
    }

    /** Draws the session as the core holds it, and leaves once it is over. A failed one stays up with its reason. */
    private suspend fun followSession() {
        combine(interactor.session(intent), start.map { it.started }.distinctUntilChanged()) { entry, _ -> entry }
            .collect(::onSession)
    }

    private fun onSession(entry: FundingBookEntry?) {
        progress.value = entry?.progress
        val session = entry?.session
        followCancel(session?.cancel)
        val stage = session?.stage
        val failure = stage?.failure?.toReason()
        if (failure != start.value.failure) start.update { it.copy(failure = failure) }

        val ended = stage?.isOpen == false
        if (start.value.started && ended && failure == null) {
            // The user accepted a cancel, but the payout had already gone through, so the withdrawal stands.
            if (progress.value?.status == FundingStatus.PaidOutAfterCancel) {
                showMessage(RCommon.string.funding_cancel_came_too_late)
            }
            closeSheet()
        }
    }

    /** A session the user left running, shown on its deposit screen as the sheet had it once it started. */
    private suspend fun resume() {
        val session = interactor.session(intent).first()?.session ?: return
        val choice = session.choice
        session.providerId?.let { loadBrands(listOf(it)) }

        flow.update { state ->
            state?.let { if (choice != null) it.withRail(choice.rail).withAsset(choice.asset) else it }
                ?.let { updated -> session.providerId?.let(updated::withChosenProvider) ?: updated }
        }
        start.update { it.copy(started = true) }
        path.value = listOf(FundingScreen.DEPOSIT)
    }

    private fun leaveCancelConfirm(message: Int) {
        if (path.value.lastOrNull() == FundingScreen.CANCEL_CONFIRM) pop()
        showMessage(message)
    }

    private fun answerCancelOffer(accept: Boolean) = launchUnit {
        if (start.value.isCancelling) return@launchUnit
        start.update { it.copy(isCancelling = true) }

        // Accepting names the terms on screen, so the core accepts nothing but the offer the user saw.
        val shown = start.value.cancel as? FundingCancel.Offered
        val terms = if (accept) shown?.let { CancelTerms(fee = it.fee, refund = it.refund) } else null

        interactor.confirmCancel(intent, terms)
            .logFailure("Funding cancel answer failed")
            .onSuccess { confirmation -> leaveCancelOffer(confirmation.message()) }
            .onFailure { leaveCancelOffer(RCommon.string.funding_error_cancel_failed) }

        start.update { it.copy(isCancelling = false) }
    }

    private fun CancelConfirmation.message(): Int? = when (this) {
        CancelConfirmation.ACCEPTED -> RCommon.string.funding_cancel_accepted
        CancelConfirmation.DECLINED -> null
        CancelConfirmation.LAPSED -> RCommon.string.funding_cancel_offer_lapsed
        CancelConfirmation.NO_OFFER -> RCommon.string.funding_cancel_offer_gone
        CancelConfirmation.TOO_LATE -> RCommon.string.funding_cancel_too_late
    }

    private fun leaveCancelOffer(message: Int?) {
        if (path.value.lastOrNull() == FundingScreen.CANCEL_OFFER) pop()
        message?.let(::showMessage)
    }

    /** Opens a new offer from the provider once, and closes the offer screen when the offer is gone. */
    private fun followCancel(cancel: FundingCancel?) {
        if (cancel == start.value.cancel) return
        start.update { it.copy(cancel = cancel) }

        val onOffer = path.value.lastOrNull() == FundingScreen.CANCEL_OFFER
        when {
            cancel is FundingCancel.Offered && !onOffer -> push(FundingScreen.CANCEL_OFFER)
            cancel !is FundingCancel.Offered && onOffer -> pop()
        }
    }

    private fun didStart(rail: FundingRail) {
        context.answer(FundingOverlayOutcome.STARTED)

        when (rail) {
            FundingRail.CARD -> closeSheet()
            FundingRail.BANK, FundingRail.CRYPTO -> if (path.value.lastOrNull() != FundingScreen.DEPOSIT) push(FundingScreen.DEPOSIT)
        }
    }

    /** The core's options; a country it detected after the sheet opened becomes the payment country until the user picks one. */
    private fun onOptions(options: FundingOptions) {
        val detected = options.detectedCountry?.takeIf { detectedCountry.value == null }?.let(FundingCountries::country)
        if (detected != null) detectedCountry.value = detected
        flow.update { state -> state?.withOptions(options)?.let { if (detected != null && it.country == null) it.withCountry(detected) else it } }
        loadBrands(options.candidates.map { it.providerId })
    }

    private fun loadBrands(providerIds: List<ProductId>) {
        providerIds.filter(brandsLoading::add).forEach { providerId ->
            launchUnit {
                val brand = interactor.brand(providerId)
                brands.update { it + (providerId to brand) }
            }
        }
    }

    /**
     * Tells the core the rail, asset, network and country on screen whenever one changes; what they lead to comes
     * back as options. The core ignores this once the sheet is no longer waited on.
     */
    private suspend fun reportSelection() {
        flow.filterNotNull().map { it.selection }.distinctUntilChanged().collect { selection ->
            context.selection = selection
            interactor.selectOptions(intent, selection)
        }
    }

    /** Quotes the new amount once the user pauses, so a provider's limit shows before Continue. */
    private fun scheduleQuote() {
        quoteJob?.cancel()
        val current = flow.value ?: return
        if (current.rail == FundingRail.CRYPTO || current.amount.signum() <= 0) return

        quoteJob = launch {
            delay(REQUOTE_DELAY)
            requestQuote()
        }
    }

    private fun requestQuote() {
        val ask = flow.value?.currentAsk() ?: return
        val number = flow.updateAndGet { it?.asking(ask) }?.askNumber ?: return

        launchUnit {
            interactor.requestQuote(intent, ask)
                .logFailure("Funding quote request failed")
                .onSuccess { id -> flow.update { it?.requested(id, number) } }
        }
    }

    @AssistedFactory
    interface Factory {
        fun create(context: FundingOverlayContext): FundingViewModel
    }
}
