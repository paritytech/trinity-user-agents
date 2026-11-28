package io.paritytech.polkadotapp.feature_products_impl.presentation.workerCrash

import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerCrashes
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class WorkerCrashNotifier @Inject constructor(
    private val crashes: WorkerCrashes,
    private val resolveProduct: ResolveProductUseCase,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
) {
    suspend fun showNotices() {
        crashes.products.collect { productId ->
            val name = resolveProduct.resolve(productId).getOrNull()?.product?.name ?: productId.value
            appNotifier.error(contextManager.applicationContext.getString(RCommon.string.worker_crashing_notice, name))
        }
    }
}
