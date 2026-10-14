package io.paritytech.polkadotapp.feature_settings_impl.presentation.main

import android.os.Build
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.settings.SettingsInteractor
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val interactor: SettingsInteractor,
    appThemeSelector: AppThemeSelector,
    private val router: SettingsRouter
) : BaseViewModel() {
    private val isLanguageSettingsAvailable = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU

    val state: StateFlow<SettingsUiState> = combine(
        interactor.observeBackupExists(),
        interactor.subscribeHasBlockedContacts(),
        interactor.observePrivacyMode(),
        appThemeSelector.selectedTheme
    ) { backupExists, hasBlockedUsers, privacyMode, selectedTheme ->
        SettingsUiState(
            debugMenuEnabled = FeatureOption.DEBUG_MENU.isEnabled,
            linkedDevicesEnabled = FeatureOption.LINKED_DEVICES.isEnabled,
            productSettingsEnabled = FeatureOption.PRODUCT_SETTINGS.isEnabled,
            isLanguageSettingsAvailable = isLanguageSettingsAvailable,
            selectedTheme = selectedTheme,
            privacyMode = privacyMode,
            isBackupMissing = !backupExists,
            hasBlockedUsers = hasBlockedUsers
        )
    }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = SettingsUiState(
                debugMenuEnabled = FeatureOption.DEBUG_MENU.isEnabled,
                linkedDevicesEnabled = FeatureOption.LINKED_DEVICES.isEnabled,
                productSettingsEnabled = FeatureOption.PRODUCT_SETTINGS.isEnabled,
                isLanguageSettingsAvailable = isLanguageSettingsAvailable,
                selectedTheme = PolkadotAppTheme.DEFAULT,
                privacyMode = null,
                isBackupMissing = false,
                hasBlockedUsers = false
            )
        )

    fun onPrivacyModeSelected(mode: RecyclingStrategyType) = launchUnit {
        interactor.setPrivacyMode(mode)
            .logFailure("Failed to change payment privacy mode")
    }

    fun onBackupClick() {
        router.openBackup()
    }

    fun onLinkedDevicesClick() {
        router.openLinkedDevices()
    }

    fun onLegalAndSupportClick() {
        router.openLegalAndSupport()
    }

    fun onProductsClick() {
        router.openProductSettings()
    }

    fun onBlockedUsersClick() {
        router.openBlockedUsers()
    }

    fun onNotificationsClick() {
        router.openNotificationSettings()
    }

    fun onLanguageClick() {
        router.openLanguageSettings()
    }

    fun onThemeClick() {
        router.openThemes()
    }

    fun onDebugMenuClick() {
        router.openDebugMenu()
    }
}
