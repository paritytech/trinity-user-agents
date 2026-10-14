package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportDisplay
import kotlinx.coroutines.flow.StateFlow

interface PairRequestContract {
    val state: StateFlow<LoadingState<PairRequestUiState>>

    val stalenessReport: StalenessReportDisplay

    fun onApproveClicked()

    fun onRejectClicked()
}

@Immutable
data class PairRequestDeviceUiModel(
    val name: String,
    val hostVersion: String?,
    val platformType: String?,
)

enum class ConnectingStep { VERIFYING, REGISTERING, SYNCING }

@Immutable
sealed interface PairRequestUiState {
    data class Confirmation(val device: PairRequestDeviceUiModel) : PairRequestUiState

    data class Connecting(
        val device: PairRequestDeviceUiModel,
        val step: ConnectingStep,
    ) : PairRequestUiState

    data class LimitReached(val totalSlots: Int) : PairRequestUiState
}
