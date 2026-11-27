package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingPhase
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingQuotes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
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

@HiltViewModel
class FundingSheetViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val sessions: FundingSessions,
    private val quotes: FundingQuotes,
    private val interactor: FundingFlowInteractor,
    private val router: ProductsRouter,
    private val cashFormat: FundingCashFormat,
    timeProvider: TimeProvider,
) : BaseViewModel(), FundingSheet {
    override val intentId = FundingIntentId(savedStateHandle.get<String>(FundingBottomSheet.SESSION_ID).orEmpty())

    private val picks = MutableStateFlow(FundingPicks.NONE)
    private val stack = MutableStateFlow<List<FundingScreenEntry>>(emptyList())
    private var visits = 0

    // One that could not close under another screen closes once back on top.
    @Volatile
    var isClosed: Boolean = false
        private set

    // Until the session ended; one gone before it ended takes the sheet down.
    override val entry: StateFlow<FundingEntry?> = flow {
        var last: FundingEntry? = null
        sessions.entryState(intentId).collect { next ->
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
            known + coroutineScope {
                ids.filterNot(known::containsKey).map { id -> async { id to interactor.brand(id) } }.awaitAll()
            }
        }
        .stateIn(this, SharingStarted.Eagerly, emptyMap())

    override val now: StateFlow<Instant> = timeProvider.ticking(1.seconds).stateIn(this, WhileOnScreen, timeProvider.now())

    val screen: StateFlow<FundingScreenEntry?> = stack.map { it.lastOrNull() }
        .stateIn(this, SharingStarted.Eagerly, null)

    init {
        sessions.shown(intentId)
        reportSelection()
    }

    override fun update(transform: (FundingPicks) -> FundingPicks) {
        picks.update(transform)
    }

    override fun requote() {
        val current = stateOf(entry.value, picks.value, wallet.value, flow.value) ?: return
        val amount = current.askAmount ?: return
        sessions.select(current.selection, intentId)
        quotes.quote(amount, intentId)
    }

    override fun push(screen: FundingScreen) {
        stack.update { it + FundingScreenEntry(screen, ++visits) }
    }

    override fun pop(screen: FundingScreen) {
        if (top() == screen) pop()
    }

    fun onBack() {
        val leavesStartedDeposit = entry.value?.hasStarted == true && top() == FundingScreen.DEPOSIT
        if (stack.value.size <= 1 || leavesStartedDeposit) close() else pop()
    }

    private fun pop() {
        stack.update { it.dropLast(1) }
    }

    override fun close() {
        sessions.leave(intentId)
        takeDown()
    }

    override fun message(text: Int) = showMessage(text)

    override fun onCleared() {
        super.onCleared()
        sessions.leave(intentId)
    }

    private fun takeDown() {
        isClosed = true
        launchUnit { router.closeFundingOverlay(intentId) }
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

    // The options freeze once the session started; a resumed session shows the rail, asset and provider it started with.
    private fun stateOf(entry: FundingEntry?, picks: FundingPicks, wallet: FundingWallet?, previous: FundingFlowState?): FundingFlowState? {
        if (entry == null || wallet == null) return null

        val options = entry.options?.takeUnless { entry.hasStarted } ?: previous?.options ?: entry.options
        val detected = options?.detectedCountry?.let(FundingCountries::country) ?: previous?.country
        val base = FundingFlowState.initial(
            direction = entry.direction,
            cashPrecision = wallet.cashPrecision,
            format = cashFormat,
            spendable = wallet.spendable.getOrNull(),
            options = options,
            amount = entry.amount,
            country = detected,
            spendableUnread = wallet.spendable.isFailure,
        )
        val choice = entry.session?.choice?.takeIf { entry.hasStarted }
        val resumed = choice?.let { base.withRail(it.rail).withAsset(it.asset) } ?: base
        val chosen = entry.session?.providerId?.takeIf { entry.hasStarted }?.let(resumed::withChosenProvider) ?: resumed
        val picked = picks.applyTo(chosen)

        val shown = previous?.let {
            picked.copy(rows = it.rows, staleQuotes = it.staleQuotes, askedAmount = it.askedAmount, list = it.list, quotesFailed = it.quotesFailed)
        }
        return (shown ?: picked).withQuotes(entry.quotes, entry.askedAmount, entry.quotesFailed)
    }

    // Card top-ups are paid in the provider's own screen. A crypto provider on another network reports no address to send to.
    private fun started(rail: FundingRail) {
        when (rail) {
            FundingRail.CARD -> close()
            FundingRail.BANK, FundingRail.CRYPTO -> if (top() != FundingScreen.DEPOSIT) push(FundingScreen.DEPOSIT)
        }
    }

    private fun followCancel(previous: FundingCancel?, cancel: FundingCancel?) {
        if (cancel == previous) return

        val onOffer = top() == FundingScreen.CANCEL_OFFER
        when {
            cancel is FundingCancel.Offered && !onOffer -> push(FundingScreen.CANCEL_OFFER)
            cancel !is FundingCancel.Offered && onOffer -> pop()
        }
    }

    private fun reportSelection() {
        flow.filterNotNull()
            .map { it.selection }
            .distinctUntilChanged()
            .onEach { selection -> if (entry.value?.hasStarted != true) sessions.select(selection, intentId) }
            .launchIn(this)
    }

    private fun top(): FundingScreen? = stack.value.lastOrNull()?.screen

    private data class FundingWallet(val cashPrecision: Int, val spendable: Result<Balance>)
}
