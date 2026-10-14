package io.paritytech.polkadotapp.app.root.navigation.username

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.LegalUrls
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.presentation.BrowserNavigator
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupFoundPayload
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import javax.inject.Inject

class UsernameNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
    private val browserNavigator: BrowserNavigator
) : BaseNavigator(navigationHolder), UsernamesRouter {
    override fun openMain() {
        performNavigation(R.id.action_global_to_main_graph)
    }

    override fun openBackupFound(payload: BackupFoundPayload) {
        performNavigation(
            actionId = R.id.action_claim_username_to_backupFoundBottomSheet,
            args = payload.toPayloadBundle()
        )
    }

    override fun openRecoverOptions() {
        performNavigation(R.id.action_claim_username_to_recoverOptionsBottomSheet)
    }

    override fun openRecoverOptionsFromClaimUnavailable() {
        performNavigation(R.id.action_claim_unavailable_to_recoverOptionsBottomSheet)
    }

    override fun openRegistrationQueue() {
        performNavigationToGraph(
            actionId = R.id.action_global_to_registration_queue,
            graphId = R.id.claim_username_graph,
            startDestinationId = R.id.registrationQueueFragment
        )
    }

    override fun openClaimUnavailable() {
        performNavigationToGraph(
            actionId = R.id.action_global_to_claim_unavailable,
            graphId = R.id.claim_username_graph,
            startDestinationId = R.id.claimUnavailableFragment
        )
    }

    override fun openIntegrityFailed() {
        performNavigationToGraph(
            actionId = R.id.action_global_to_integrity_failed,
            graphId = R.id.claim_username_graph,
            startDestinationId = R.id.integrityFailedFragment
        )
    }

    override fun openTermsOfUse() {
        browserNavigator.open(LegalUrls.TERMS_OF_USE)
    }

    override fun openPrivacyPolicy() {
        browserNavigator.open(LegalUrls.PRIVACY_POLICY)
    }
}
