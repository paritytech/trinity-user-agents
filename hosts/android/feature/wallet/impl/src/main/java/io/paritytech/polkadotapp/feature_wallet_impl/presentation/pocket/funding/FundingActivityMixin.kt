package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

/** The funding activity and sheets a wallet screen shows over the core's sessions. */
class FundingActivityMixin private constructor(
    private val hostFunding: HostFunding,
    private val chainAssetProvider: ChainAssetProvider,
    private val tokenAmountMapper: TokenAmountMapper,
    private val scope: CoroutineScope,
) {
    class Factory @Inject constructor(
        private val hostFunding: HostFunding,
        @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
        private val tokenAmountMapper: TokenAmountMapper,
    ) {
        fun create(scope: CoroutineScope) = FundingActivityMixin(hostFunding, chainAssetProvider, tokenAmountMapper, scope)
    }

    private val sheets = FundingSheets()
    private val opening = MutableStateFlow(false)

    val activity: StateFlow<FundingActivityUiState> = hostFunding.observeActivity()
        .map { FundingActivityMapper(chainAssetProvider.asset(), tokenAmountMapper).map(it) }
        .stateIn(scope, SharingStarted.WhileSubscribed(STOP_TIMEOUT_MS), EMPTY)

    val sheet: StateFlow<FundingSheet?> = sheets.shown.stateIn(scope, SharingStarted.Eagerly, null)

    init {
        acknowledgeShownHistory()
    }

    fun openSheet(sheet: FundingSheet) = sheets.open(sheet)

    fun back() = sheets.back()

    fun close() = sheets.close()

    /** A session in flight opens where it stands; an ended one opens its detail. */
    fun onRowClick(id: String) {
        sheets.close()
        scope.launch { hostFunding.showSession(id) }
    }

    /** Opens one session at a time: a second tap while the overlay is up is ignored. */
    fun open(direction: FundingDirection, onFailure: (Throwable) -> Unit) {
        if (opening.value) return
        opening.enable()
        scope.launch {
            hostFunding.openFunding(direction, amount = null).onFailure(onFailure)
            opening.disable()
        }
    }

    /** The core keeps an ended session owed to the host until the user has seen it in the history. */
    private fun acknowledgeShownHistory() = scope.launch {
        sheet
            .flatMapLatest { shown ->
                if (shown == FundingSheet.HISTORY) activity.map { it.completed.map { row -> row.id } } else emptyFlow()
            }
            .distinctUntilChanged()
            .filter { it.isNotEmpty() }
            .collect { hostFunding.acknowledgeShown(it) }
    }

    private companion object {
        const val STOP_TIMEOUT_MS = 5_000L
        val EMPTY = FundingActivityUiState(inFlight = persistentListOf(), completed = persistentListOf())
    }
}
