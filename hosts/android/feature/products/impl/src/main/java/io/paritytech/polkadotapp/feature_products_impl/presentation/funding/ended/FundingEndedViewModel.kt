package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCashFormat
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSessionPresenter
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.FundingDirection
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

data class FundingEndedScreenState(
    val ended: FundingEndedUiState,
    val direction: FundingDirection,
    val providerName: String?,
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
) : BaseViewModel() {
    private val intent = savedStateHandle.get<String>(FundingEndedBottomSheet.INTENT).orEmpty()

    val state: StateFlow<LoadingState<FundingEndedScreenState>> = flow {
        val summary = service.entryState(intent).firstOrNull()?.summary
        val ended = summary?.let { FundingEndedMapper(interactor.cash(), cashFormat).map(it) }
        if (summary == null || ended == null) {
            close()
            return@flow
        }
        emit(LoadingState.Loaded(FundingEndedScreenState(ended, summary.direction, providerName = null)))
        summary.providerId?.let { providerId ->
            emit(LoadingState.Loaded(FundingEndedScreenState(ended, summary.direction, interactor.brand(providerId).name)))
        }
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

    private fun close() = launchUnit { router.closeFundingEnded(intent) }
}
