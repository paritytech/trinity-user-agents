package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.utils.launchAsyncJob

class ProductNotificationBootReceiver : BroadcastReceiver() {
    @EntryPoint
    @InstallIn(SingletonComponent::class)
    interface Dependencies {
        fun scheduler(): ProductNotificationScheduler
        fun runtimeSettings(): io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
        fun runtimeProvider(): io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        // Resolved lazily rather than via @AndroidEntryPoint: the generated injection runs before the
        // action check and blows up when the broadcast reaches a process whose graph is not built yet
        // (HiltTestApplication under instrumentation). Restoring notifications is best-effort — skip instead.
        val dependencies = runCatching {
            EntryPointAccessors.fromApplication(context.applicationContext, Dependencies::class.java)
        }.getOrNull() ?: return

        launchAsyncJob {
            runCatching {
                if (dependencies.runtimeSettings().isTrUAPIRuntimeEnabled()) {
                    dependencies.runtimeProvider().runtime().getOrThrow().reconcileNotifications()
                } else {
                    dependencies.scheduler().restoreAll().getOrThrow()
                }
            }.onFailure { timber.log.Timber.e(it, "Notification reconciliation failed") }
        }
    }
}
