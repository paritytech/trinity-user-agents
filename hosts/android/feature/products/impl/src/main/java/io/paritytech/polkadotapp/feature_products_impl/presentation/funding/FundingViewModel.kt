package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.asLoaded
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
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
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.moneyMoved
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Whether the session has been handed to a provider, and why it failed once it has. */
data class FundingStartState(
    val isStarting: Boolean,
    val failed: Boolean,
    val started: Boolean,
    val failure: FundingFailureReason? = null,
    val isCancelling: Boolean = false,
    /** The user's cancel as the core holds it, once money has moved and the provider decides it. */
    val cancel: FundingCancel? = null,
    /** Money moved for the session, so only the provider can end it; before, a second cancel ends it. */
    val moneyMoved: Boolean = false,
)

@HiltViewModel
class FundingViewModel @Inject constructor(
    private val context: FundingOverlayContext,
    private val contexts: FundingOverlayContexts,
    private val interactor: FundingFlowInteractor,
    private val router: ProductsRouter,
    private val clipboard: ClipboardService,
) : BaseViewModel() {
    private companion object {
        val REQUOTE_DELAY = 600.milliseconds
        val DEPOSIT_POLL = 3.seconds
    }

    private val intent = context.request.intent
    private val flow = MutableStateFlow<FundingFlowState?>(null)
    private val path = MutableStateFlow<List<FundingScreen>>(emptyList())
    private val start = MutableStateFlow(FundingStartState(isStarting = false, failed = false, started = false))
    private val progress = MutableStateFlow<FundingProgress?>(null)
    private var quoteJob: Job? = null
    private var startsWhenQuoted = false

    private val brands = MutableStateFlow<Map<String, FundingProviderBrand>>(emptyMap())
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
        launch { interactor.sessionChanges(intent).collect { refreshSession() } }
        launchUnit {
            val initial = initialState()
            flow.value = initial
            loadBrands(initial)
            refreshAmountLimits()
            if (context.mode == FundingOverlayMode.Resumed) resume()
        }
        launch { path.map { it.lastOrNull() == FundingScreen.DEPOSIT }.distinctUntilChanged().collectLatest { if (it) followDeposit() } }
    }

    fun onNetworkChosen(id: String) {
        flow.update { it?.withNetwork(id) }
        push(FundingScreen.TOKEN)
    }

    fun onOpenNetworks() {
        push(FundingScreen.NETWORK)
        launchUnit { refreshNetworkMinimums() }
    }

    /** Value in goes straight to the deposit screen, which starts with the best quote once every provider answered. */
    fun onTokenChosen(symbol: String) {
        val current = flow.value ?: return
        flow.value = current.withAsset(symbol)
        requestQuote()
        launchUnit { refreshAmountLimits() }

        if (current.direction == CoreFundingDirection.IN) {
            startsWhenQuoted = true
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
        refreshSession()
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
        launchUnit { refreshAmountLimits() }
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
        launchUnit { refreshCountrySupport() }
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
        launchUnit { refreshAmountLimits() }
    }

    fun onProviderChosen(providerId: String) {
        flow.update { it?.withChosenProvider(providerId) }
        showMessage(RCommon.string.funding_provider_changed)
        pop()
    }

    /** Hands the session to the provider whose quote is on screen, then answers `Started`. */
    fun onStart() {
        val current = flow.value ?: return
        val providerId = current.selectedProviderId ?: return
        if (start.value.isStarting || start.value.started) return

        start.value = FundingStartState(isStarting = true, failed = false, started = false)
        launchUnit {
            val selected = interactor.selectProvider(intent, providerId, current.rows[providerId]?.quote?.quoteId)
                .logFailure("Funding provider selection failed")
                .getOrDefault(false)

            start.value = FundingStartState(isStarting = false, failed = !selected, started = selected)
            if (selected) didStart(current.rail) else refreshSession()
        }
    }

    fun onBack() {
        val leavesStartedDeposit = start.value.started && path.value.lastOrNull() == FundingScreen.DEPOSIT
        if (path.value.isEmpty() || leavesStartedDeposit) onClose() else pop()
    }

    /** Before `Started` leaving is the answer the core discards the session on; after it the session runs on. */
    fun onClose() {
        quoteJob?.cancel()
        stopQuotes()
        context.answer(FundingOverlayOutcome.DISMISSED)
        closeSheet()
    }

    override fun onCleared() {
        super.onCleared()
        context.answer(FundingOverlayOutcome.DISMISSED)
        context.markClosed()
        contexts.remove(intent)
    }

    private fun closeSheet() = launchUnit { router.closeFundingOverlay(intent) }

    // The core asks every provider again each minute until told to stop. It stops on its own once a provider is
    // chosen or the session ends, so only leaving the sheet with the session still open needs this.
    private fun stopQuotes() = launchUnit { interactor.stopQuotes(intent) }

    private suspend fun initialState(): FundingFlowState {
        val cash = interactor.cash()
        val request = context.request
        val detected = interactor.detectedCountry()
        detectedCountry.value = detected

        return FundingFlowState.initial(
            direction = request.direction.toCore(),
            cash = cash,
            spendable = interactor.spendable(cash),
            candidates = interactor.candidates(intent),
            amount = request.amount?.let { cash.decimal(it.value.toString()) },
            country = detected,
        )
    }

    private fun push(screen: FundingScreen) {
        path.update { it + screen }
    }

    private fun pop() {
        path.update { it.dropLast(1) }
    }

    private fun onQuoteList(list: FundingQuoteList) {
        val current = flow.value ?: return
        val updated = current.receiving(list)
        if (updated === current) return
        flow.value = updated
        // What the answers showed about each provider is learned by the core, so the limits may have moved.
        if (updated.allAnswered) {
            launchUnit {
                refreshCountrySupport()
                refreshAmountLimits()
            }
        }

        if (startsWhenQuoted && updated.allAnswered) {
            startsWhenQuoted = false
            onStart()
        }
    }

    /** The core's view of the session moved on: read it again, and leave once it is over. A failed one stays up with its reason. */
    private suspend fun refreshSession() {
        progress.value = interactor.progress(intent)
        val session = interactor.session(intent)
        followCancel(session?.cancel)
        val moneyMoved = session?.moneyMoved == true
        if (moneyMoved != start.value.moneyMoved) start.update { it.copy(moneyMoved = moneyMoved) }
        val stage = session?.stage
        val failure = stage?.failure?.toReason()
        if (failure != start.value.failure) start.update { it.copy(failure = failure) }

        val ended = stage?.isOpen == false
        if (start.value.started && ended && failure == null) closeSheet()
    }

    /** A session the user left running, shown on its deposit screen as the sheet had it once it started. */
    private suspend fun resume() {
        val session = interactor.session(intent) ?: return
        val choice = session.choice

        flow.update { state ->
            state?.let { if (choice != null) it.withRail(choice.rail).withAsset(choice.asset) else it }
                ?.let { updated -> session.providerId?.let(updated::withChosenProvider) ?: updated }
        }
        start.value = FundingStartState(isStarting = false, failed = false, started = true)
        path.value = listOf(FundingScreen.DEPOSIT)
        refreshSession()
    }

    private fun leaveCancelConfirm(message: Int) {
        if (path.value.lastOrNull() == FundingScreen.CANCEL_CONFIRM) pop()
        showMessage(message)
    }

    private fun answerCancelOffer(accept: Boolean) = launchUnit {
        if (start.value.isCancelling) return@launchUnit
        start.update { it.copy(isCancelling = true) }

        interactor.confirmCancel(intent, accept)
            .logFailure("Funding cancel answer failed")
            .onSuccess { leaveCancelOffer(it.message()) }
            .onFailure { leaveCancelOffer(RCommon.string.funding_error_cancel_failed) }

        start.update { it.copy(isCancelling = false) }
        refreshSession()
    }

    private fun CancelConfirmation.message(): Int? = when (this) {
        CancelConfirmation.ACCEPTED -> RCommon.string.funding_cancel_accepted
        CancelConfirmation.DECLINED -> null
        CancelConfirmation.LAPSED -> RCommon.string.funding_cancel_offer_lapsed
        CancelConfirmation.NO_OFFER -> RCommon.string.funding_cancel_offer_gone
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

    private suspend fun followDeposit() {
        while (true) {
            if (start.value.started) refreshSession()
            delay(DEPOSIT_POLL)
        }
    }

    private fun didStart(rail: FundingRail) {
        context.answer(FundingOverlayOutcome.STARTED)
        launchUnit { refreshSession() }

        when (rail) {
            FundingRail.CARD -> closeSheet()
            FundingRail.BANK, FundingRail.CRYPTO -> if (path.value.lastOrNull() != FundingScreen.DEPOSIT) push(FundingScreen.DEPOSIT)
        }
    }

    private fun loadBrands(state: FundingFlowState) {
        state.candidates.forEach { candidate ->
            launchUnit {
                val brand = interactor.brand(candidate.providerId)
                brands.update { it + (candidate.providerId to brand) }
            }
        }
    }

    /** What the providers take for the rail, asset, network and country on screen, as the core works it out. */
    private suspend fun refreshAmountLimits() {
        val current = flow.value ?: return
        val asset = current.asset ?: current.fiatAsset
        val limits = interactor.amountLimits(intent, current.cash, current.rail, asset, current.network, current.country?.code)
        flow.update { it?.withAmountLimits(limits) }
    }

    /** The least each crypto network takes: the lowest minimum across its tokens. */
    private suspend fun refreshNetworkMinimums() {
        val current = flow.value ?: return
        val minimums = current.networks.associate { network ->
            val assets = current.cryptoRoutes
                .filter { route -> route.networks?.contains(network.id) ?: true }
                .flatMap { it.assets }
                .distinct()
            val minimum = assets
                .mapNotNull { asset ->
                    interactor.amountLimits(intent, current.cash, FundingRail.CRYPTO, asset, network.id, current.country?.code).minimum
                }
                .minOrNull()
            network.id to minimum
        }
        flow.update { state -> state?.withNetworkMinimums(minimums.mapNotNull { (id, min) -> min?.let { id to it } }.toMap()) }
    }

    /** What the core now knows about each country for the rail and asset on screen. */
    private suspend fun refreshCountrySupport() {
        val current = flow.value ?: return
        val asset = current.ask?.asset ?: current.asset ?: current.fiatAsset
        val support = interactor.countrySupport(intent, current.rail, asset)
        flow.update { it?.withCountrySupport(support) }
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
        flow.update { it?.asking(ask) }

        launchUnit {
            interactor.requestQuote(intent, ask)
                .logFailure("Funding quote request failed")
                .onSuccess { id -> flow.update { it?.requested(id) } }
        }
    }
}
