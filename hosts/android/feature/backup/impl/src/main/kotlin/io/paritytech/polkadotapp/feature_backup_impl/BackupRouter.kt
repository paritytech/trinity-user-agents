package io.paritytech.polkadotapp.feature_backup_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter

interface BackupRouter : ReturnableRouter {
    fun openSyncSettings()

    fun openRecoverMnemonic()
}
