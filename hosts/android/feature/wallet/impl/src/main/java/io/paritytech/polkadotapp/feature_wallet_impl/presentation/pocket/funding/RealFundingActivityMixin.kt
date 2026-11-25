package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.GetCashUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.WithdrawUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

internal class RealFundingActivityMixin(
    private val hostFunding: HostFunding,
    private val chainAssetProvider: ChainAssetProvider,
    private val tokenAmountMapper: TokenAmountMapper,
    private val timeProvider: TimeProvider,
    private val scope: CoroutineScope,
    private val onFailure: (PresentationThrowable) -> Unit,
) : FundingActivityMixin {
    class Factory @Inject constructor(
        private val hostFunding: HostFunding,
        @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
        private val tokenAmountMapper: TokenAmountMapper,
        private val timeProvider: TimeProvider,
    ) : FundingActivityMixin.Factory {
        override fun create(scope: CoroutineScope, onFailure: (PresentationThrowable) -> Unit): FundingActivityMixin =
            RealFundingActivityMixin(hostFunding, chainAssetProvider, tokenAmountMapper, timeProvider, scope, onFailure)
    }

    private val sheets = FundingSheets()
    private val opening = MutableStateFlow(false)
    private val historyScreen = MutableStateFlow(false)

    // Never ends: the core's sessions as they change, with the clock.
    private val observed: SharedFlow<FundingActivityUiState> =
        combine(hostFunding.observeActivity(), timeProvider.ticking(NOW_TICK)) { activity, now ->
            FundingActivityMapper(chainAssetProvider.asset(), tokenAmountMapper).map(activity, now)
        }.shareIn(scope, SharingStarted.WhileSubscribed(STOP_TIMEOUT_MS), replay = 1)

    override val activity: StateFlow<FundingActivityUiState> =
        observed.stateIn(scope, SharingStarted.WhileSubscribed(STOP_TIMEOUT_MS), empty())

    override val sheet: StateFlow<FundingSheet?> = sheets.shown.stateIn(scope, SharingStarted.Eagerly, null)

    init {
        acknowledgeShownHistory()
    }

    // Waits for the first list, so a tap before it arrives still shows the sessions in progress.
    override fun onStart(direction: FundingDirection) = whileOpening {
        val current = observed.first()
        val inProgress = if (direction == FundingDirection.IN) current.toppingUp else current.sending
        if (inProgress.isEmpty()) openFunding(direction) else sheets.open(direction.inProgressSheet())
    }

    override fun onStartNew(direction: FundingDirection) {
        sheets.close()
        open(direction)
    }

    override fun onHistory() = sheets.open(FundingSheet.HISTORY)

    override fun onHistoryShown() {
        historyScreen.value = true
    }

    override fun onSheetBack() = sheets.back()

    override fun onSheetDismiss() = sheets.close()

    override fun onRowClick(id: String) {
        sheets.close()
        scope.launch { hostFunding.showSession(id) }
    }

    private fun open(direction: FundingDirection) = whileOpening { openFunding(direction) }

    // One session at a time: a second tap while the overlay is up is ignored.
    private fun whileOpening(action: suspend () -> Unit) {
        if (opening.value) return
        opening.enable()
        scope.launch {
            action()
            opening.disable()
        }
    }

    private suspend fun openFunding(direction: FundingDirection) {
        hostFunding.isFundingAvailable()
            .flatMap { available -> if (available) hostFunding.openFunding(direction, amount = null) else Result.failure(NoFundingProvider()) }
            .onFailure { onFailure(direction.unavailable(it)) }
    }

    // The core keeps an ended session owed to the host until the user has seen it in the history.
    private fun acknowledgeShownHistory() = scope.launch {
        combine(sheet, historyScreen) { shown, screen -> shown == FundingSheet.HISTORY || screen }
            .distinctUntilChanged()
            .flatMapLatest { onScreen ->
                if (onScreen) observed.map { it.completed.map { row -> row.id } } else emptyFlow()
            }
            .distinctUntilChanged()
            .filter { it.isNotEmpty() }
            .collect { hostFunding.acknowledgeShown(it) }
    }

    private fun empty() = FundingActivityUiState(inFlight = persistentListOf(), completed = persistentListOf(), now = timeProvider.now())

    private fun FundingDirection.inProgressSheet() = if (this == FundingDirection.IN) FundingSheet.TOPPING_UP else FundingSheet.SENDING

    private fun FundingDirection.unavailable(cause: Throwable): PresentationThrowable = when (this) {
        FundingDirection.IN -> GetCashUnavailablePresentationError(cause)
        FundingDirection.OUT -> WithdrawUnavailablePresentationError(cause)
    }

    private class NoFundingProvider : IllegalStateException("No funding provider is listed")

    private companion object {
        const val STOP_TIMEOUT_MS = 5_000L

        // Ended rows read their time as "today" or a weekday, so a minute is fine enough.
        val NOW_TICK = 1.minutes
    }
}
