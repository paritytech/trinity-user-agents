package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingPhase
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionId
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
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
    timeProvider: TimeProvider,
) : BaseViewModel(), FundingSheet {
    override val sessionId = FundingSessionId(savedStateHandle.get<String>(FundingBottomSheet.SESSION_ID).orEmpty())

    private val picks = MutableStateFlow(FundingPicks.NONE)
    private val stack = MutableStateFlow<List<FundingScreenEntry>>(emptyList())
    private var visits = 0

    /** Set once the sheet should be gone; one that could not close under another screen closes once back on top. */
    @Volatile
    var isClosed: Boolean = false
        private set

    // Until the session ended; one gone before it ended takes the sheet down.
    override val entry: StateFlow<FundingEntry?> = flow {
        var last: FundingEntry? = null
        service.entryState(sessionId).collect { next ->
            onEntry(previous = last, entry = next)
            last = next
            emit(next)
        }
        if (last?.hasEnded != true) takeDown()
    }.stateIn(this, SharingStarted.Eagerly, null)

    private val wallet: StateFlow<FundingWallet?> = flow {
        emit(FundingWallet(interactor.cashPrecision(), interactor.spendable()))
    }.stateIn(this, SharingStarted.Eagerly, null)

    override val flow: StateFlow<FundingFlowState?> = combine(entry, picks, wallet, ::Triple)
        .scan(null as FundingFlowState?) { previous, (entry, picks, wallet) -> stateOf(entry, picks, wallet, previous) }
        .stateIn(this, SharingStarted.Eagerly, null)

    override val brands: StateFlow<Map<ProductId, FundingProviderBrand>> = entry.filterNotNull()
        .map { it.options?.candidates.orEmpty().map { candidate -> candidate.providerId } + listOfNotNull(it.session?.providerId) }
        .distinctUntilChanged()
        .scan(emptyMap<ProductId, FundingProviderBrand>()) { known, ids ->
            known + ids.filterNot(known::containsKey).associateWith { interactor.brand(it) }
        }
        .stateIn(this, SharingStarted.Eagerly, emptyMap())

    override val now: StateFlow<Instant> = timeProvider.ticking(1.seconds).stateIn(this, WhileOnScreen, timeProvider.now())

    /** The screen on top; `null` until the session first arrives. */
    val screen: StateFlow<FundingScreenEntry?> = stack.map { it.lastOrNull() }
        .stateIn(this, SharingStarted.Eagerly, null)

    init {
        service.shown(sessionId)
        reportSelection()
    }

    override fun update(transform: (FundingPicks) -> FundingPicks) {
        picks.update(transform)
    }

    override fun requote() {
        val current = stateOf(entry.value, picks.value, wallet.value, flow.value) ?: return
        val amount = current.askAmount ?: return
        service.select(current.selection, sessionId)
        service.quote(amount, sessionId)
    }

    override fun push(screen: FundingScreen) {
        stack.update { it + FundingScreenEntry(screen, ++visits) }
    }

    override fun pop(screen: FundingScreen) {
        if (top() == screen) pop()
    }

    /** Back walks the sheet's own screens; from its first screen, or a started session's deposit, it leaves. */
    fun onBack() {
        val leavesStartedDeposit = entry.value?.hasStarted == true && top() == FundingScreen.DEPOSIT
        if (stack.value.size <= 1 || leavesStartedDeposit) close() else pop()
    }

    private fun pop() {
        stack.update { it.dropLast(1) }
    }

    override fun close() {
        service.leave(sessionId)
        takeDown()
    }

    override fun message(text: Int) = showMessage(text)

    override fun onCleared() {
        super.onCleared()
        service.leave(sessionId)
    }

    private fun takeDown() {
        isClosed = true
        launchUnit { router.closeFundingOverlay(sessionId) }
    }

    private fun onEntry(previous: FundingEntry?, entry: FundingEntry) {
        if (previous == null) push(if (entry.hasStarted) FundingScreen.DEPOSIT else FundingScreen.AMOUNT)
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

    /**
     * The session as the core holds it with the user's picks over it. The options stop changing once the session
     * started; a session the user comes back to shows the rail, asset and provider it was started with.
     */
    private fun stateOf(entry: FundingEntry?, picks: FundingPicks, wallet: FundingWallet?, previous: FundingFlowState?): FundingFlowState? {
        if (entry == null || wallet == null) return null

        val options = entry.options?.takeUnless { entry.hasStarted } ?: previous?.options ?: entry.options
        val detected = options?.detectedCountry?.let(FundingCountries::country) ?: previous?.country
        val base = FundingFlowState.initial(
            direction = entry.direction,
            cashPrecision = wallet.cashPrecision,
            format = cashFormat,
            spendable = wallet.spendable,
            options = options,
            amount = entry.amount,
            country = detected,
        )
        val choice = entry.session?.choice?.takeIf { entry.hasStarted }
        val resumed = choice?.let { base.withRail(it.rail).withAsset(it.asset) } ?: base
        val chosen = entry.session?.providerId?.takeIf { entry.hasStarted }?.let(resumed::withChosenProvider) ?: resumed
        val picked = picks.applyTo(chosen)

        val shown = previous?.let { picked.copy(rows = it.rows, staleQuotes = it.staleQuotes, askedAmount = it.askedAmount, list = it.list) }
        return (shown ?: picked).withQuotes(entry.quotes, entry.askedAmount)
    }

    /**
     * Card top-ups are paid in the provider's own screen, so the sheet closes. The others follow the session on the
     * deposit screen, which shows where to send once the provider reports it: a crypto provider serving another network
     * than the app's own takes the payment in its own screen and reports none.
     */
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
    private fun reportSelection() {
        flow.filterNotNull()
            .map { it.selection }
            .distinctUntilChanged()
            .onEach { selection -> if (entry.value?.hasStarted != true) service.select(selection, sessionId) }
            .launchIn(this)
    }

    private fun top(): FundingScreen? = stack.value.lastOrNull()?.screen

    private data class FundingWallet(val cashPrecision: Int, val spendable: Balance?)
}
