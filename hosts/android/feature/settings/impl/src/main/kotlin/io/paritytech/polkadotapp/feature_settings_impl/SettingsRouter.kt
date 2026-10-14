package io.paritytech.polkadotapp.feature_settings_impl

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupConflictPayload

interface SettingsRouter : ReturnableRouter {
    fun openBackup()
    fun openMnemonic()
    fun openBackupConflict(payload: BackupConflictPayload)
    fun openDebugMenu()
    fun openCurrency()
    fun openProductSettings()
    fun openBlockedUsers()
    fun openLinkedDevices()
    fun openDeviceDetails(deviceId: String)
    fun openScanQr()
    fun openContactChat(accountId: AccountId)
    fun openNotificationSettings()
    fun openLanguageSettings()
    fun openLegalAndSupport()
    fun openPrivacyPolicy()
    fun openTermsOfUse()
    fun openContactUs()
    fun openThemes()

    // TODO: should not be here, remove after W3S
    fun openClaimUsername()
}
