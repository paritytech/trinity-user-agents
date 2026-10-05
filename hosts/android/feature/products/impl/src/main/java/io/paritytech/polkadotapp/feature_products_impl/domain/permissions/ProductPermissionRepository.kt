package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.database.dao.ProductPermissionGrantDao
import io.paritytech.polkadotapp.database.model.ProductPermissionGrantLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCoreRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toProductPermissions
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

interface ProductPermissionRepository {
    suspend fun isGranted(productId: ProductId, permission: ProductPermission): Boolean

    suspend fun isAnyGranted(productId: ProductId, permissionType: String, permissionKeys: List<String>): Boolean

    suspend fun grant(productId: ProductId, permission: ProductPermission)

    fun grantOneTime(productId: ProductId, permission: ProductPermission)

    fun consumeOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean

    fun hasOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean

    suspend fun revoke(productId: ProductId, permission: ProductPermission)

    suspend fun getAllByProduct(productId: ProductId): List<ProductPermissionStatus>

    fun observeAllByProduct(productId: ProductId): Flow<List<ProductPermissionStatus>>

    fun observeHasAnyPermissionRequested(productId: ProductId): Flow<Boolean>

    suspend fun revokeAllByProduct(productId: ProductId)
}

class RealProductPermissionRepository @Inject constructor(
    private val dao: ProductPermissionGrantDao,
    private val runtimeSettings: io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings,
    private val runtimeProvider: dagger.Lazy<io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider>,
) : ProductPermissionRepository {
    private suspend fun runtime() = runtimeProvider.get().runtime().getOrThrow()

    private val oneTimeGrants = ConcurrentHashMap.newKeySet<String>()

    private fun oneTimeGrantKey(productId: ProductId, permission: ProductPermission): String {
        return "${productId.value}:${permission.typeName}:${permission.key}"
    }

    override fun grantOneTime(productId: ProductId, permission: ProductPermission) {
        check(!runtimeSettings.isTrUAPIRuntimeEnabled()) { "Rust owns one-use authorizations" }
        oneTimeGrants.add(oneTimeGrantKey(productId, permission))
    }

    override fun consumeOneTimeGrant(productId: ProductId, permission: ProductPermission): Boolean {
        check(!runtimeSettings.isTrUAPIRuntimeEnabled()) { "Rust owns one-use authorizations" }
        return oneTimeGrants.remove(oneTimeGrantKey(productId, permission))
    }

    override fun hasOneTimeGrant(
        productId: ProductId,
        permission: ProductPermission
    ): Boolean {
        check(!runtimeSettings.isTrUAPIRuntimeEnabled()) { "Rust owns one-use authorizations" }
        return oneTimeGrantKey(productId, permission) in oneTimeGrants
    }

    override suspend fun isGranted(productId: ProductId, permission: ProductPermission): Boolean {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtime().permissionAuthorizationStatus(productId.value, permission.toCoreRequest()) == uniffi.truapi.PermissionAuthorizationStatus.AUTHORIZED
        val grant = dao.get(productId.value, permission.typeName, permission.key)
        return grant?.granted == true
    }

    override suspend fun isAnyGranted(productId: ProductId, permissionType: String, permissionKeys: List<String>): Boolean {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return permissionKeys.any { isGranted(productId, ProductPermission.fromLocal(permissionType, it)) }
        return dao.isAnyGranted(productId.value, permissionType, permissionKeys)
    }

    override suspend fun grant(productId: ProductId, permission: ProductPermission) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().setPermissionAuthorizationStatus(productId.value, permission.toCoreRequest(), uniffi.truapi.PermissionAuthorizationStatus.AUTHORIZED)
            runtimeProvider.get().notifyRecordsChanged()
            return
        }
        dao.insert(
            ProductPermissionGrantLocal(
                productId = productId.value,
                permissionType = permission.typeName,
                permissionKey = permission.key,
                granted = true,
                grantedAt = System.currentTimeMillis(),
            )
        )
    }

    override suspend fun revoke(productId: ProductId, permission: ProductPermission) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().setPermissionAuthorizationStatus(productId.value, permission.toCoreRequest(), uniffi.truapi.PermissionAuthorizationStatus.DENIED)
            runtimeProvider.get().notifyRecordsChanged()
            return
        }
        dao.revoke(productId.value, permission.typeName, permission.key)
    }

    override suspend fun getAllByProduct(productId: ProductId): List<ProductPermissionStatus> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtime().permissions().filter { it.productId == productId.value }.flatMap { it.toProductPermissions() }
        return dao.getAllByProduct(productId.value).map { it.toDomain() }
    }

    override fun observeAllByProduct(productId: ProductId): Flow<List<ProductPermissionStatus>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtimeProvider.get().observeRecords(emptyList()) { getAllByProduct(productId) }
        return dao.observeAllByProduct(productId.value)
            .map { grants -> grants.map { it.toDomain() } }
    }

    override fun observeHasAnyPermissionRequested(productId: ProductId): Flow<Boolean> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return observeAllByProduct(productId).map { it.isNotEmpty() }
        return dao.observeHasAnyPermissionRequested(productId.value)
    }

    private fun ProductPermissionGrantLocal.toDomain(): ProductPermissionStatus {
        return ProductPermissionStatus(
            permission = ProductPermission.fromLocal(permissionType, permissionKey),
            granted = granted
        )
    }

    override suspend fun revokeAllByProduct(productId: ProductId) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().permissions().filter { it.productId == productId.value }.forEach {
                runtime().setPermissionAuthorizationStatus(productId.value, it.request, uniffi.truapi.PermissionAuthorizationStatus.NOT_DETERMINED)
            }
            runtimeProvider.get().notifyRecordsChanged()
            return
        }
        dao.deleteAllByProduct(productId.value)
    }
}
