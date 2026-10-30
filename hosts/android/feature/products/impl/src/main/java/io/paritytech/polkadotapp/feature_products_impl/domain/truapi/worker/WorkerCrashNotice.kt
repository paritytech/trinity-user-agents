package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBranding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Tells the user a product's worker keeps crashing, once the core says so. The core goes on restarting it with
 * backoff, so the notice is transient and asks nothing. The product is named as the funding sheet names a provider,
 * by the bare id until its manifest resolves.
 */
@Singleton
class WorkerCrashNotice @Inject constructor(
    // Lazy: resolving a manifest reaches the runtime, whose bridge reports crashes here.
    private val branding: Lazy<FundingProviderBranding>,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
    dispatchers: CoroutineDispatchers,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    /** May arrive on any thread; the name is resolved off it. */
    fun onCrashing(productId: String) {
        scope.launch {
            val name = runCatching { branding.get().brand(productId).name }.getOrDefault(productId)
            appNotifier.error(contextManager.applicationContext.getString(RCommon.string.worker_crashing_notice, name))
        }
    }
}
