package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.ProductPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

interface ProductPermissionGuard {
    /**
     * Requests the given permission from the user.
     * If the permission is already permanently granted, returns true immediately.
     * Otherwise, prompts the user for a decision (AllowOnce, AllowAlways, Deny).
     *
     * Use this for host API calls that explicitly request a permission (e.g. `host_device_permission`).
     * One-time grants issued here can later be consumed by [consumePermission].
     */
    suspend fun requestPermission(productId: ProductId, permission: ProductPermission): Boolean

    suspend fun requestPermissionsBatched(productId: ProductId, permissions: List<RemotePermission>): Boolean

    /**
     * Consumes a previously issued one-time grant, or falls back to [requestPermission] if none exists.
     *
     * Use this for host API calls that require a permission to perform an action (e.g. `host_push_notification`).
     * The expected flow is: the product first calls a permission-request API (which issues a one-time grant via
     * [requestPermission]), then calls the action API (which consumes it here without re-prompting the user).
     * If no one-time grant is available, the user is prompted as a fallback.
     */
    suspend fun consumePermission(productId: ProductId, permission: ProductPermission): Boolean

    /**
     * Read-only query: returns whether the permission is currently granted, either via an
     * unconsumed one-time grant or a permanent grant. Never prompts the user and never
     * consumes a one-time grant.
     *
     * Use this to silently gate behaviour; use [requestPermission] / [consumePermission]
     * when a missing permission should trigger a prompt.
     */
    suspend fun check(productId: ProductId, permission: ProductPermission): Boolean
}

class RealProductPermissionGuard @Inject constructor(
    private val remotePermissionHandler: ProductPermissionHandler<RemotePermission>,
    private val accountAccessHandler: ProductPermissionHandler<ProductPermission.AccountAccess>,
    private val balanceAccessHandler: ProductPermissionHandler<ProductPermission.BalanceAccess>,
    private val deviceCapabilityHandler: ProductPermissionHandler<ProductPermission.DeviceCapability>,
    private val userIdentityAccessHandler: ProductPermissionHandler<ProductPermission.UserIdentityAccess>,
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
) : ProductPermissionGuard {
    private val requestMutex = Mutex()

    override suspend fun requestPermission(productId: ProductId, permission: ProductPermission): Boolean {
        if (check(productId, permission)) return true

        return requestMutex.withLock {
            requestUnderLock(productId, permission)
        }
    }

    override suspend fun requestPermissionsBatched(
        productId: ProductId,
        permissions: List<RemotePermission>,
    ): Boolean {
        if (permissions.isEmpty()) return true

        val notYetGranted = permissions.filterNot { check(productId, it) }.distinct()
        if (notYetGranted.isEmpty()) return true

        return requestMutex.withLock {
            // A concurrent request may have granted some of these while we waited for the lock
            val stillNotGranted = notYetGranted.filterNot { check(productId, it) }
            if (stillNotGranted.isEmpty()) return@withLock true

            when (requester.promptBatched(productId, stillNotGranted)) {
                PermissionDecision.AllowAlways -> {
                    stillNotGranted.forEach { repository.grant(productId, it) }
                    true
                }
                PermissionDecision.AllowOnce -> {
                    stillNotGranted.forEach { repository.grantOneTime(productId, it) }
                    true
                }
                PermissionDecision.Deny -> false
            }
        }
    }

    override suspend fun consumePermission(productId: ProductId, permission: ProductPermission): Boolean {
        if (repository.consumeOneTimeGrant(productId, permission)) return true

        return requestMutex.withLock {
            // A concurrent request may have issued a one-time grant while we waited for the lock
            if (repository.consumeOneTimeGrant(productId, permission)) return@withLock true

            requestUnderLock(productId, permission).also {
                // Consume the one-time grant the request might have just issued
                repository.consumeOneTimeGrant(productId, permission)
            }
        }
    }

    override suspend fun check(productId: ProductId, permission: ProductPermission): Boolean {
        if (repository.hasOneTimeGrant(productId, permission)) return true

        return when (permission) {
            is RemotePermission -> remotePermissionHandler.isGranted(productId, permission)
            is ProductPermission.AccountAccess -> accountAccessHandler.isGranted(productId, permission)
            is ProductPermission.BalanceAccess -> balanceAccessHandler.isGranted(productId, permission)
            is ProductPermission.DeviceCapability -> deviceCapabilityHandler.isGranted(productId, permission)
            is ProductPermission.UserIdentityAccess -> userIdentityAccessHandler.isGranted(productId, permission)
        }
    }

    /**
     * Performs the actual permission request. Must be called while [requestMutex] is held so that
     * only one request prompt is in flight at a time. [check] runs lock-free and may be called concurrently.
     */
    private suspend fun requestUnderLock(productId: ProductId, permission: ProductPermission): Boolean {
        // A concurrent request may have granted this while we waited for the lock
        if (check(productId, permission)) return true

        return when (permission) {
            is RemotePermission -> remotePermissionHandler.request(productId, permission)
            is ProductPermission.AccountAccess -> accountAccessHandler.request(productId, permission)
            is ProductPermission.BalanceAccess -> balanceAccessHandler.request(productId, permission)
            is ProductPermission.DeviceCapability -> deviceCapabilityHandler.request(productId, permission)
            is ProductPermission.UserIdentityAccess -> userIdentityAccessHandler.request(productId, permission)
        }
    }
}
