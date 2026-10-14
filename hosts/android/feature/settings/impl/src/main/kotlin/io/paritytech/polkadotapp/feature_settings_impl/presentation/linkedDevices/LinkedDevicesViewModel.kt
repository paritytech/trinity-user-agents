package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.linkedDevices.LinkedDevicesInteractor
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDeviceCategory
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDeviceUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesSectionUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesUiState
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.presentation.formatHostVersionLabel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toPersistentList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class LinkedDevicesViewModel @Inject constructor(
    private val router: SettingsRouter,
    interactor: LinkedDevicesInteractor
) : BaseViewModel(), LinkedDevicesContract {
    override val state: StateFlow<LinkedDevicesUiState> = interactor.observeLinkedDevices()
        .map { sessions -> LinkedDevicesUiState(sections = sessions.toSections()) }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = LinkedDevicesUiState()
        )

    override fun onBackClick() {
        router.back()
    }

    override fun onAddDeviceClick() {
        router.openScanQr()
    }

    override fun onHowItWorksClick() {
        // TODO design is not ready
    }

    override fun onDeviceClick(deviceId: String) {
        router.openDeviceDetails(deviceId)
    }

    private fun List<ActiveSsoSession>.toSections(): ImmutableList<LinkedDevicesSectionUiModel> {
        if (isEmpty()) return persistentListOf()

        return map { it.toUi() }
            .groupBy { it.category }
            .map { (category, devices) ->
                LinkedDevicesSectionUiModel(category, devices.toPersistentList())
            }
            .toPersistentList()
    }

    private fun ActiveSsoSession.toUi(): LinkedDeviceUiModel {
        return LinkedDeviceUiModel(
            id = id,
            name = name,
            description = hostVersion?.let(::formatHostVersionLabel),
            category = LinkedDeviceCategory.LAPTOP_DESKTOP
        )
    }
}
