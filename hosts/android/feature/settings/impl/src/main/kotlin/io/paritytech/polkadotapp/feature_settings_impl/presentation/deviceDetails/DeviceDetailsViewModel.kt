package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.deviceDetails.DeviceDetailsInteractor
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.models.DeviceDetailsUiState
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.presentation.formatHostVersionLabel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class DeviceDetailsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val interactor: DeviceDetailsInteractor,
    private val router: SettingsRouter,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
    private val timeFormatter: TimeFormatter,
) : BaseViewModel(), DeviceDetailsContract {
    private val payload: DeviceDetailsPayload = savedStateHandle.getPayload()

    private val isRemoving = MutableStateFlow(false)
    private val isRemoveConfirmationVisible = MutableStateFlow(false)

    private val deviceFlow = interactor.observeDevice(payload.deviceId)

    override val state: StateFlow<LoadingState<DeviceDetailsUiState>> = combine(
        deviceFlow,
        isRemoving,
        isRemoveConfirmationVisible,
    ) { device, removing, confirmationVisible ->
        device?.toUiState(removing, confirmationVisible)
    }
        .mapNotNull { it }
        .withLoading("DeviceDetails")
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onBackClick() {
        router.back()
    }

    override fun onRemoveDeviceClick() {
        if (isRemoving.value) return
        isRemoveConfirmationVisible.value = true
    }

    override fun onCancelRemoveClick() {
        if (isRemoving.value) return
        isRemoveConfirmationVisible.value = false
    }

    override fun onConfirmRemoveClick() {
        if (isRemoving.value) return

        launch {
            val device = deviceFlow.mapNotNull { it }.first()

            isRemoving.value = true
            interactor.removeDevice(device.statementAccountId)
                .onSuccess { router.back() }
                .onFailure {
                    isRemoving.value = false
                    isRemoveConfirmationVisible.value = false
                    appNotifier.error(contextManager.applicationContext.getString(RCommon.string.generic_error_notification))
                }
        }
    }

    private fun ActiveSsoSession.toUiState(isRemoving: Boolean, confirmationVisible: Boolean): DeviceDetailsUiState {
        val deviceLabel = platformType
        val hostLabel = if (hostVersion != null) "$name ${formatHostVersionLabel(hostVersion!!)}" else name
        val addedLabel = timeFormatter.formatChatDateSeparator(addedAt, relativeTo = System.currentTimeMillis())

        return DeviceDetailsUiState(
            deviceLabel = deviceLabel,
            hostLabel = hostLabel,
            addedLabel = addedLabel,
            isRemoving = isRemoving,
            isRemoveConfirmationVisible = confirmationVisible,
        )
    }
}
