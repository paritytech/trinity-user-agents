package io.paritytech.polkadotapp.app.root.domain.debug

import android.app.ActivityManager
import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_splash_api.domain.DevResetCoordinator
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import timber.log.Timber
import javax.inject.Inject

class RealDevResetCoordinator @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val backupService: BackupService,
    private val runtimeSettings: io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings,
    private val runtimeProvider: io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider,
) : DevResetCoordinator {
    override suspend fun clearAllAndClose() {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) runtimeProvider.resetAccount()
        backupService.deleteBackup()
            .onFailure { Timber.w(it, "Failed to delete cloud backup during dev reset") }

        val activityManager = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        activityManager.clearApplicationUserData()
    }
}
