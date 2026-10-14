package io.paritytech.polkadotapp.feature_usernames_impl.presentation

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupFoundPayload

interface UsernamesRouter : ReturnableRouter {
    fun openBackupFound(payload: BackupFoundPayload)

    fun openMain()

    fun openRecoverOptions()

    fun openRecoverOptionsFromClaimUnavailable()

    fun openRegistrationQueue()

    fun openClaimUnavailable()

    fun openIntegrityFailed()

    fun openTermsOfUse()

    fun openPrivacyPolicy()
}
