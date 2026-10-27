package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import dagger.Lazy
import io.paritytech.polkadotapp.database.dao.ProductPermissionGrantDao
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionDeniedException
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.NetworkAccessPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.normalizeProductId
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.truapi.PermissionAuthorizationEntry
import uniffi.truapi.PermissionAuthorizationStatus
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

interface ProductPermissionRepository {
    suspend fun isGranted(productId: ProductId, permission: ProductPermission): Boolean
    suspend fun isDenied(productId: ProductId, permission: ProductPermission): Boolean
    suspend fun isAnyGranted(productId: ProductId, permissionType: String, permissionKeys: List<String>): Boolean
    suspend fun grant(productId: ProductId, permission: ProductPermission)
    suspend fun grantOneTime(productId: ProductId, permission: ProductPermission)
    suspend fun consumeOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean
    suspend fun hasOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean
    suspend fun revoke(productId: ProductId, permission: ProductPermission)
    suspend fun getAllByProduct(productId: ProductId): List<ProductPermissionStatus>
    fun observeAllByProduct(productId: ProductId): Flow<List<ProductPermissionStatus>>
    fun observeHasAnyPermissionRequested(productId: ProductId): Flow<Boolean>
    suspend fun revokeAllByProduct(productId: ProductId)
    suspend fun withPermissionRequest(productId: ProductId, block: suspend () -> Boolean): Boolean
}

class RealProductPermissionRepository @Inject constructor(
    private val dao: ProductPermissionGrantDao,
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val changes: PermissionAuthorizationChanges,
) : ProductPermissionRepository {
    private data class GrantKey(val product: String, val permission: ProductPermission)
    private val oneTimeGrants = ConcurrentHashMap<GrantKey, Long>()
    private val mutationMutex = Mutex()

    private class RequestRevision(val product: String, val core: ULong) :
        AbstractCoroutineContextElement(Key) {
        companion object Key : CoroutineContext.Key<RequestRevision>
    }

    private suspend fun runtime() = runtimeProvider.get().runtime().getOrThrow()
    private fun id(productId: ProductId) = normalizeProductId(productId.value)
    private fun key(productId: ProductId, permission: ProductPermission) = GrantKey(
        id(productId), permission.canonicalRequest().legacyPermission(),
    )

    override suspend fun withPermissionRequest(
        productId: ProductId,
        block: suspend () -> Boolean,
    ): Boolean {
        // Import every legacy grant, including balance access, before capturing the core revision.
        getAllByProduct(productId)
        val product = id(productId)
        val revision = RequestRevision(product, runtime().permissionAuthorizationRevision(product))
        return withContext(revision) {
            try {
                block() && revision.core == runtime().permissionAuthorizationRevision(product)
            } catch (_: ProductPermissionDeniedException) {
                false
            }
        }
    }

    private suspend fun checkRevision(productId: ProductId, permission: ProductPermission) {
        val revision = currentCoroutineContext()[RequestRevision] ?: return
        check(revision.product == id(productId))
        if (revision.core != runtime().permissionAuthorizationRevision(revision.product)) {
            throw ProductPermissionDeniedException(permission)
        }
    }

    override suspend fun grantOneTime(productId: ProductId, permission: ProductPermission) = mutationMutex.withLock {
        val localRevision = changes.revision(id(productId))
        if (isDenied(productId, permission)) throw ProductPermissionDeniedException(permission)
        checkRevision(productId, permission)
        oneTimeGrants[key(productId, permission)] = localRevision
    }

    override suspend fun consumeOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean {
        if (!hasOneTimeGrant(productId, permission)) return false
        val revision = oneTimeGrants.remove(key(productId, permission)) ?: return false
        return revision == changes.revision(id(productId))
    }

    override suspend fun hasOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean {
        val revision = oneTimeGrants[key(productId, permission)] ?: return false
        val denied = isDenied(productId, permission)
        return !denied && revision == changes.revision(id(productId))
    }

    override suspend fun isGranted(productId: ProductId, permission: ProductPermission): Boolean {
        return effectiveStatuses(productId, canonicalEntries(productId))[key(productId, permission)] ==
            PermissionAuthorizationStatus.AUTHORIZED
    }

    override suspend fun isDenied(productId: ProductId, permission: ProductPermission): Boolean {
        val records = effectiveStatuses(productId, canonicalEntries(productId))
        val candidates = if (permission is ProductPermission.RemotePermission.NetworkAccess) {
            NetworkAccessPermissionHandler.generateDomainCandidates(permission.domain).map {
                ProductPermission.RemotePermission.NetworkAccess(it)
            }
        } else listOf(permission)
        return candidates.firstNotNullOfOrNull { records[key(productId, it)] } == PermissionAuthorizationStatus.DENIED
    }

    override suspend fun isAnyGranted(productId: ProductId, permissionType: String, permissionKeys: List<String>): Boolean {
        // A specific deny beats a broader wildcard grant.
        val permissions = permissionKeys.map { ProductPermission.fromLocal(permissionType, it) }
        val records = effectiveStatuses(productId, canonicalEntries(productId))
        return permissions.firstNotNullOfOrNull { records[key(productId, it)] } == PermissionAuthorizationStatus.AUTHORIZED
    }

    override suspend fun grant(productId: ProductId, permission: ProductPermission) = set(productId, permission, true)
    override suspend fun revoke(productId: ProductId, permission: ProductPermission) = set(productId, permission, false)

    private suspend fun set(productId: ProductId, permission: ProductPermission, granted: Boolean) = mutationMutex.withLock {
        checkRevision(productId, permission)
        val product = id(productId)
        val request = permission.canonicalRequest()
        val revision = currentCoroutineContext()[RequestRevision]
        if (!granted) {
            changes.changed(product)
            oneTimeGrants.keys.removeAll { it.product == product }
        }
        val runtime = runtime()
        val status = if (granted) PermissionAuthorizationStatus.AUTHORIZED else PermissionAuthorizationStatus.DENIED
        if (revision == null) {
            runtime.setPermissionAuthorizationStatus(product, request, status)
        } else if (!runtime.setPermissionAuthorizationStatusIfCurrent(product, request, status, revision.core)) {
            throw ProductPermissionDeniedException(permission)
        }
        // Conditional authorization preserves the core revision. Never recapture it here:
        // an external revoke must remain visible to the next write in a batched prompt.
    }

    override suspend fun getAllByProduct(productId: ProductId): List<ProductPermissionStatus> {
        val legacy = dao.getAllByProduct(productId.value).map {
            ProductPermissionStatus(ProductPermission.fromLocal(it.permissionType, it.permissionKey), it.granted)
        }
        val canonical = canonicalEntries(productId, legacy)
        val result = linkedMapOf<ProductPermission, ProductPermissionStatus>()
        canonical.forEach { entry ->
            val permission = entry.request.legacyPermission()
            result[permission] = ProductPermissionStatus(permission, entry.status == PermissionAuthorizationStatus.AUTHORIZED)
        }
        return result.values.toList()
    }

    private suspend fun canonicalEntries(
        productId: ProductId,
        legacy: List<ProductPermissionStatus>? = null,
    ): List<PermissionAuthorizationEntry> {
        val rows = legacy ?: dao.getAllByProduct(productId.value).map {
            ProductPermissionStatus(ProductPermission.fromLocal(it.permissionType, it.permissionKey), it.granted)
        }
        val imports = rows.map { row ->
            PermissionAuthorizationEntry(
                row.permission.canonicalRequest(),
                if (row.granted) PermissionAuthorizationStatus.AUTHORIZED else PermissionAuthorizationStatus.DENIED,
            )
        }
        val runtime = runtime()
        return if (imports.isEmpty()) runtime.permissionAuthorizations(id(productId))
            else runtime.importPermissionAuthorizations(id(productId), imports)
    }

    private fun effectiveStatuses(
        productId: ProductId,
        entries: List<PermissionAuthorizationEntry>,
    ): Map<GrantKey, PermissionAuthorizationStatus> = entries.associate { entry ->
        key(productId, entry.request.legacyPermission()) to entry.status
    }

    override fun observeAllByProduct(productId: ProductId): Flow<List<ProductPermissionStatus>> =
        combine(dao.observeAllByProduct(productId.value), changes.observe(id(productId))) { _, _ -> getAllByProduct(productId) }

    override fun observeHasAnyPermissionRequested(productId: ProductId): Flow<Boolean> =
        observeAllByProduct(productId).map { it.isNotEmpty() }

    override suspend fun revokeAllByProduct(productId: ProductId) {
        getAllByProduct(productId).forEach { revoke(productId, it.permission) }
    }
}
