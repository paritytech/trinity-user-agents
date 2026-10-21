package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionPromptUnavailableException
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * Shows the permission prompt and waits for the answer of the user.
 *
 * A request can come from a product with no screen, such as a chat worker. When no prompt can get an answer, the
 * request fails with [PermissionPromptUnavailableException] and does not wait. A wait with no prompt on the screen
 * holds [mutex] and the lock of the core, so it stops all later requests.
 */
@Singleton
class RealProductPermissionRequester(
    private val permissionContextHolder: PermissionContextHolder,
    private val productsRouter: ProductsRouter,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val foregroundWait: Duration,
) : ProductPermissionRequester {
    @Inject
    constructor(
        permissionContextHolder: PermissionContextHolder,
        productsRouter: ProductsRouter,
        appLifecycleObserver: AppLifecycleObserver,
    ) : this(permissionContextHolder, productsRouter, appLifecycleObserver, FOREGROUND_WAIT)

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
        awaitForeground()
        val context = ProductPermissionContext(productId, permissions)
        try {
            permissionContextHolder.set(context)
            // A prompt that does not show gets no answer, and the wait would hold every later request.
            if (!productsRouter.openPermissionPrompt(context.id)) {
                Timber.w("No screen shows the permission prompt of %s", productId.value)
                throw PermissionPromptUnavailableException("the permission prompt cannot show")
            }
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

    // The wait lets a product that starts with the app ask when the first screen shows.
    private suspend fun awaitForeground() {
        withTimeoutOrNull(foregroundWait) {
            appLifecycleObserver.subscribe().first { it == AppLifecycleState.FOREGROUND }
        } ?: throw PermissionPromptUnavailableException("the app is not in the foreground")
    }

    private companion object {
        val FOREGROUND_WAIT = 5.seconds
    }
}
