package io.paritytech.polkadotapp.app.root.navigation.settings

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.LegalUrls
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.app.root.navigation.SupportContact
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.BrowserNavigator
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.openAppLanguageSettings
import io.paritytech.polkadotapp.common.utils.openAppNotificationSettings
import io.paritytech.polkadotapp.common.utils.openEmail
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupConflictPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.BackupConflictBottomSheet
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.DeviceDetailsFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.DeviceDetailsPayload
import javax.inject.Inject

class SettingsNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
    @param:ApplicationContext private val context: Context,
    private val browserNavigator: BrowserNavigator,
    private val contextManager: ContextManager
) : BaseNavigator(navigationHolder), SettingsRouter {
    override fun openBackup() {
        performNavigation(R.id.action_global_to_backup_graph)
    }

    override fun openMnemonic() {
        performNavigation(R.id.action_BackupStatusFragment_to_mnemonicRevealFragment)
    }

    override fun openBackupConflict(payload: BackupConflictPayload) {
        performNavigation(
            actionId = R.id.action_BackupStatusFragment_to_backupConflictBottomSheet,
            args = BackupConflictBottomSheet.createBundle(payload)
        )
    }

    override fun openDebugMenu() {
        performNavigation(R.id.action_global_to_debug_menu)
    }

    override fun openCurrency() {
        performNavigation(R.id.action_global_to_change_currency)
    }

    override fun openProductSettings() {
        performNavigation(R.id.action_global_to_product_settings_graph)
    }

    override fun openBlockedUsers() {
        performNavigation(R.id.action_global_to_blockedUsersFragment)
    }

    override fun openLinkedDevices() {
        performNavigation(R.id.action_global_to_linkedDevicesFragment)
    }

    override fun openDeviceDetails(deviceId: String) {
        performNavigation(
            actionId = R.id.action_global_to_deviceDetailsFragment,
            args = DeviceDetailsFragment.createBundle(DeviceDetailsPayload(deviceId)),
        )
    }

    override fun openScanQr() {
        if (isCurrentDestination(R.id.scanQrFragment)) return
        performNavigation(R.id.action_global_to_scan_graph)
    }

    override fun openContactChat(accountId: AccountId) {
        performNavigation(
            actionId = R.id.action_global_to_chatFeedFragment,
            args = ChatFeedPayload.existingContactChat(accountId).toPayloadBundle()
        )
    }

    override fun openNotificationSettings() {
        context.openAppNotificationSettings()
    }

    override fun openLanguageSettings() {
        context.openAppLanguageSettings()
    }

    override fun openLegalAndSupport() {
        performNavigation(R.id.action_global_to_legalAndSupportFragment)
    }

    override fun openPrivacyPolicy() {
        browserNavigator.open(LegalUrls.PRIVACY_POLICY)
    }

    override fun openTermsOfUse() {
        browserNavigator.open(LegalUrls.TERMS_OF_USE)
    }

    override fun openContactUs() {
        contextManager.requireActivity().openEmail(SupportContact.EMAIL)
    }

    override fun openThemes() {
        performNavigation(R.id.action_global_to_change_theme)
    }

    override fun openClaimUsername() {
        performNavigationToGraph(
            actionId = R.id.action_global_to_claim_username_graph,
            graphId = R.id.claim_username_graph,
            startDestinationId = R.id.claimUsernameFragment
        )
    }
}
