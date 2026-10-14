package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealProductPermissionRequester @Inject constructor(
    private val permissionContextHolder: PermissionContextHolder,
    private val productsRouter: ProductsRouter,
) : ProductPermissionRequester {
    private val mutex = Mutex()

    override suspend fun prompt(
        productId: ProductId,
        permission: ProductPermission,
    ): PermissionDecision = doPrompt(productId, listOf(permission))

    override suspend fun promptBatched(
        productId: ProductId,
        permissions: List<RemotePermission>,
    ): PermissionDecision {
        require(permissions.isNotEmpty()) { "promptBatched called with no permissions" }
        return doPrompt(productId, permissions)
    }

    private suspend fun doPrompt(
        productId: ProductId,
        permissions: List<ProductPermission>,
    ): PermissionDecision = mutex.withLock {
        val context = ProductPermissionContext(productId, permissions)
        try {
            permissionContextHolder.set(context)
            productsRouter.openPermissionPrompt(context.id)
            context.awaitDecision()
        } finally {
            withContext(NonCancellable) {
                try {
                    productsRouter.closePermissionPrompt(context.id)
                } finally {
                    permissionContextHolder.clear()
                }
            }
        }
    }
}
