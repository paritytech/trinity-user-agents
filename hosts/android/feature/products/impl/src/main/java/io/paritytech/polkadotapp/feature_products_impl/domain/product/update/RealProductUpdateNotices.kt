package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.domain.product.PendingProductUpdate
import io.paritytech.polkadotapp.feature_products_api.domain.product.ProductUpdateNotices
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.domain.product.productIconUri
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealProductUpdateNotices @Inject constructor(
    private val productUpdater: ProductUpdater,
    private val productsInUse: ProductsInUse,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val dotNsTldProvider: DotNsTldProvider,
) : ProductUpdateNotices {
    private val dismissedNames = MutableStateFlow<Set<String>>(emptySet())

    override fun observeNotice(): Flow<PendingProductUpdate?> {
        return combine(
            productUpdater.observePendingNames(),
            productsInUse.observeShownPages(),
            dismissedNames,
        ) { pendingNames, shownPages, dismissed ->
            shownPages.firstOrNull { page ->
                page.dotNsName in pendingNames && page.dotNsName !in dismissed && !isBuiltIn(page.productId)
            }
        }
            .distinctUntilChanged()
            .map { page -> page?.let { noticeOf(it) } }
    }

    override suspend fun applyUpdate(pendingUpdate: PendingProductUpdate) {
        productUpdater.applyUpdateAndReloadPages(pendingUpdate.dotNsName)
    }

    override fun dismiss(pendingUpdate: PendingProductUpdate) {
        dismissedNames.update { it + pendingUpdate.dotNsName }
    }

    fun start(scope: CoroutineScope) {
        productUpdater.observeFoundUpdates()
            .onEach { foundNames -> dismissedNames.update { it - foundNames } }
            .launchIn(scope)
    }

    private suspend fun isBuiltIn(productId: ProductId): Boolean {
        return productId in ReservedProductIds.all(dotNsTldProvider.getTldRetrying())
    }

    private suspend fun noticeOf(page: ShownPage): PendingProductUpdate {
        val productName = resolveProductUseCase.resolve(page.productId)
            .logFailure("Failed to read the name of ${page.productId} for its update notice")
            .map { it.product.name }
            .getOrDefault(page.productId.value)

        return PendingProductUpdate(
            productName = productName,
            iconUri = page.productId.productIconUri(),
            dotNsName = page.dotNsName,
        )
    }
}
