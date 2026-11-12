package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingPhase
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The funding sheet of one session: which of its screens is on top, what the user picked, and how the sheet follows the
 * session. A sheet whose session is gone, because the core dropped it or it was restored without one, closes itself.
 */
@HiltViewModel
class FundingSheetViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val service: FundingService,
    private val interactor: FundingFlowInteractor,
    private val router: ProductsRouter,
    private val cashFormat: FundingCashFormat,
) : BaseViewModel(), FundingSheet {
    override val intent: String = savedStateHandle.get<String>(FundingBottomSheet.INTENT).orEmpty()

    private val entryState = MutableStateFlow<FundingEntry?>(null)
    private val draft = MutableStateFlow<FundingFlowState?>(null)
    private val brandsState = MutableStateFlow<Map<ProductId, FundingProviderBrand>>(emptyMap())
    private val brandsLoading = mutableSetOf<ProductId>()
    private val stack = MutableStateFlow<List<FundingScreenEntry>>(emptyList())
    private var visits = 0

    /** Set once the sheet should be gone; one that could not close under another screen closes once back on top. */
    @Volatile
    var isClosed: Boolean = false
        private set

    override val entry: StateFlow<FundingEntry?> = entryState.asStateFlow()
    override val flow: StateFlow<FundingFlowState?> = draft.asStateFlow()
    override val brands: StateFlow<Map<ProductId, FundingProviderBrand>> = brandsState.asStateFlow()

    /** The screen on top; `null` until the session first arrives. */
    val screen: StateFlow<FundingScreenEntry?> = stack.map { it.lastOrNull() }
        .stateIn(this, SharingStarted.Eagerly, null)

    init {
        launchUnit { follow() }
        launch { reportSelection() }
    }

    override fun update(transform: (FundingFlowState) -> FundingFlowState) {
        draft.update { it?.let(transform) }
    }

    override fun requote() {
        val current = draft.value ?: return
        val amount = current.askAmount ?: return
        service.select(current.selection, intent)
        service.quote(amount, intent)
    }

    override fun push(screen: FundingScreen) {
        stack.update { it + FundingScreenEntry(screen, ++visits) }
    }

    override fun pop(screen: FundingScreen) {
        if (top() == screen) pop()
    }

    /** Back walks the sheet's own screens; from its first screen, or a started session's deposit, it leaves. */
    fun onBack() {
        val leavesStartedDeposit = entryState.value?.hasStarted == true && top() == FundingScreen.DEPOSIT
        if (stack.value.size <= 1 || leavesStartedDeposit) close() else pop()
    }

    private fun pop() {
        if (top() == FundingScreen.DEPOSIT && entryState.value?.hasStarted != true) service.stopStarting(intent)
        stack.update { it.dropLast(1) }
    }

    override fun close() {
        service.leave(intent)
        takeDown()
    }

    override fun message(text: Int) = showMessage(text)

    override fun onCleared() {
        super.onCleared()
        service.leave(intent)
    }

    /** Follows the session until it ended; one gone before it ended takes the sheet down. */
    private suspend fun follow() {
        service.entryState(intent).collect(::onEntry)
        if (entryState.value?.hasEnded != true) takeDown()
    }

    private fun takeDown() {
        isClosed = true
        launchUnit { router.closeFundingOverlay(intent) }
    }

    private suspend fun onEntry(entry: FundingEntry) {
        val previous = entryState.value
        if (draft.value == null) draft.value = initial(entry)
        entryState.value = entry

        draft.update { state ->
            state?.let { withFacts(it, entry) }
        }
        loadBrands(entry.options?.candidates.orEmpty().map { it.providerId } + listOfNotNull(entry.session?.providerId))
        followCancel(previous?.session?.cancel, entry.session?.cancel)

        if (previous != null && previous.phase == FundingPhase.SELECTING && entry.phase == FundingPhase.RUNNING) {
            entry.session?.choice?.rail?.let(::started)
        }
        if (previous?.hasStarted == true && entry.hasEnded && entry.failure == null) {
            // The user accepted a cancel, but the payout had already gone through, so the withdrawal stands.
            if (entry.progress?.status == FundingStatus.PaidOutAfterCancel) showMessage(RCommon.string.funding_cancel_came_too_late)
            close()
        }
    }

    /** The sheet as it opens: on the amount for a new session, on its deposit for one the user comes back to. */
    private suspend fun initial(entry: FundingEntry): FundingFlowState {
        val cash = interactor.cash()
        val detected = entry.options?.detectedCountry?.let(FundingCountries::country)
        val state = FundingFlowState.initial(
            direction = entry.direction,
            cash = cash,
            format = cashFormat,
            spendable = interactor.spendable(cash),
            options = entry.options,
            amount = entry.amount?.let(cash::decimal),
            country = detected,
        )
        val choice = entry.session?.choice
        if (!entry.hasStarted) {
            push(FundingScreen.AMOUNT)
            return state
        }

        push(FundingScreen.DEPOSIT)
        val resumed = if (choice != null) state.withRail(choice.rail).withAsset(choice.asset) else state
        return entry.session?.providerId?.let(resumed::withChosenProvider) ?: resumed
    }

    /** The core's options and quotes; a country it detected after the sheet opened is the payment country until the user picks one. */
    private fun withFacts(state: FundingFlowState, entry: FundingEntry): FundingFlowState {
        val options = entry.options
        val withOptions = if (options != null && !entry.hasStarted) state.withOptions(options) else state
        val detected = options?.detectedCountry?.takeIf { withOptions.country == null }?.let(FundingCountries::country)
        return (detected?.let(withOptions::withCountry) ?: withOptions)
            .withQuotes(entry.quotes, entry.staleQuotes, entry.isQuoting)
    }

    /** Card top-ups are paid in the provider's own screen, so the sheet closes; the others show where to send. */
    private fun started(rail: FundingRail) {
        when (rail) {
            FundingRail.CARD -> close()
            FundingRail.BANK, FundingRail.CRYPTO -> if (top() != FundingScreen.DEPOSIT) push(FundingScreen.DEPOSIT)
        }
    }

    /** Opens a new offer from the provider once, and closes the offer screen when the offer is gone. */
    private fun followCancel(previous: FundingCancel?, cancel: FundingCancel?) {
        if (cancel == previous) return

        val onOffer = top() == FundingScreen.CANCEL_OFFER
        when {
            cancel is FundingCancel.Offered && !onOffer -> push(FundingScreen.CANCEL_OFFER)
            cancel !is FundingCancel.Offered && onOffer -> pop()
        }
    }

    /**
     * Tells the core the rail, asset, network and country on screen whenever one changes; what they lead to comes back
     * as options. The core ignores this once the session is no longer set up.
     */
    private suspend fun reportSelection() {
        draft.filterNotNull().map { it.selection }.distinctUntilChanged().collect { selection ->
            if (entryState.value?.hasStarted != true) service.select(selection, intent)
        }
    }

    private fun loadBrands(providerIds: List<ProductId>) {
        providerIds.filter(brandsLoading::add).forEach { providerId ->
            launchUnit {
                val brand = interactor.brand(providerId)
                brandsState.update { it + (providerId to brand) }
            }
        }
    }

    private fun top(): FundingScreen? = stack.value.lastOrNull()?.screen
}
