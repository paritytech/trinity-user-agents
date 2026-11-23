package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionId
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCashFormat
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSessionPresenter
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.FundingDirection
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

data class FundingEndedScreenState(
    val ended: FundingEndedUiState,
    val direction: FundingDirection,
    val providerName: String?,
    val now: Instant,
)

@HiltViewModel
class FundingEndedViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val interactor: FundingFlowInteractor,
    private val service: FundingService,
    private val presenter: FundingSessionPresenter,
    private val router: ProductsRouter,
    private val clipboard: ClipboardService,
    private val cashFormat: FundingCashFormat,
    private val timeProvider: TimeProvider,
) : BaseViewModel() {
    private val sessionId = FundingSessionId(savedStateHandle.get<String>(FundingEndedBottomSheet.SESSION_ID).orEmpty())

    private val details = flow {
        val summary = service.entryState(sessionId).firstOrNull()?.summary
        val ended = summary?.let { FundingEndedMapper(interactor.cashPrecision(), cashFormat).map(it) }
        if (summary == null || ended == null) {
            close()
            return@flow
        }
        emit(Details(ended, summary.direction, providerName = null))
        summary.providerId?.let { providerId ->
            emit(Details(ended, summary.direction, interactor.brand(providerId).name))
        }
    }

    val state: StateFlow<LoadingState<FundingEndedScreenState>> = combine(details, timeProvider.ticking(NOW_TICK)) { details, now ->
        LoadingState.Loaded(FundingEndedScreenState(details.ended, details.direction, details.providerName, now))
    }.stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    fun onClose() = close()

    fun onStartOver() {
        val direction = (state.value as? LoadingState.Loaded)?.data?.direction ?: return
        close()
        presenter.startOver(direction)
    }

    fun onCopy(value: String) {
        clipboard.setPrimaryClip(value)
        showMessage(RCommon.string.funding_deposit_copied)
    }

    private fun close() = launchUnit { router.closeFundingEnded(sessionId) }

    private data class Details(val ended: FundingEndedUiState, val direction: FundingDirection, val providerName: String?)

    private companion object {
        // The times read as "today" or a weekday, so a minute is fine enough.
        val NOW_TICK = 1.minutes
    }
}
