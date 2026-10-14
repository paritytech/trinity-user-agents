package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.notification.success
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.findInCauseChain
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReport
import io.paritytech.polkadotapp.common.utils.progressStallReport.launchWithDiagnostics
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.slotAllocator.NoFreeStmtStoreSlotsException
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import io.paritytech.polkadotapp.feature_sso_impl.SsoRouter
import io.paritytech.polkadotapp.feature_sso_impl.domain.pairRequest.PairRequestInteractor
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class PairRequestViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val router: SsoRouter,
    private val interactor: PairRequestInteractor,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
) : BaseViewModel(), PairRequestContract {
    private val payload: PairRequestPayload = savedStateHandle.getPayload()
    private val offer: HandshakeOffer = payload.toDomain()

    override val stalenessReport = StalenessReport(this)

    private val phase = MutableStateFlow<Phase>(Phase.Confirm)

    override val state: StateFlow<LoadingState<PairRequestUiState>> = phase
        .map<Phase, LoadingState<PairRequestUiState>> { currentPhase ->
            val device = buildDeviceUiModel()
            val uiState = when (currentPhase) {
                Phase.Confirm -> PairRequestUiState.Confirmation(device)
                is Phase.Connecting -> PairRequestUiState.Connecting(device, currentPhase.step)
                is Phase.LimitReached -> PairRequestUiState.LimitReached(currentPhase.totalSlots)
            }
            LoadingState.Loaded(uiState)
        }
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onApproveClicked() {
        if (phase.value !is Phase.Confirm) return

        launchWithDiagnostics(stalenessReport) {
            phase.value = Phase.Connecting(ConnectingStep.VERIFYING)

            interactor.approveHandshake(offer).collect { progress ->
                when (progress) {
                    DeviceOnboardingProgress.Verifying -> phase.value = Phase.Connecting(ConnectingStep.VERIFYING)
                    DeviceOnboardingProgress.Registering -> phase.value = Phase.Connecting(ConnectingStep.REGISTERING)
                    DeviceOnboardingProgress.Syncing -> phase.value = Phase.Connecting(ConnectingStep.SYNCING)
                    DeviceOnboardingProgress.Done -> {
                        appNotifier.success(contextManager.applicationContext.getString(RCommon.string.device_connected_notification))
                        router.back()
                    }
                    is DeviceOnboardingProgress.Failed -> handleApproveFailure(progress.error)
                }
            }
        }
    }

    override fun onRejectClicked() {
        router.back()
    }

    private fun handleApproveFailure(throwable: Throwable) {
        val limitException = throwable.findInCauseChain<NoFreeStmtStoreSlotsException>()
        if (limitException != null) {
            phase.value = Phase.LimitReached(limitException.totalSlots)
        } else {
            phase.value = Phase.Confirm
            appNotifier.error(contextManager.applicationContext.getString(RCommon.string.generic_error_notification))
        }
    }

    private fun buildDeviceUiModel(): PairRequestDeviceUiModel {
        return PairRequestDeviceUiModel(
            name = offer.metadata.hostName.orEmpty(),
            hostVersion = offer.metadata.hostVersion,
            platformType = offer.metadata.platformType,
        )
    }

    private sealed interface Phase {
        data object Confirm : Phase
        data class Connecting(val step: ConnectingStep) : Phase
        data class LimitReached(val totalSlots: Int) : Phase
    }
}
