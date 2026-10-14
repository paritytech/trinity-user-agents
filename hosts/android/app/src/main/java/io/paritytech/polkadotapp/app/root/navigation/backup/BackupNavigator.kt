package io.paritytech.polkadotapp.app.root.navigation.backup

import android.content.Context
import android.content.Intent
import android.provider.Settings
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import javax.inject.Inject

class BackupNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
    @param:ApplicationContext
    private val context: Context
) : BaseNavigator(navigationHolder), BackupRouter {
    override fun openSyncSettings() {
        val intent = Intent(Settings.ACTION_SYNC_SETTINGS).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
    }

    override fun openRecoverMnemonic() {
        performNavigation(R.id.action_recover_options_to_recoverMnemonicFragment)
    }
}
