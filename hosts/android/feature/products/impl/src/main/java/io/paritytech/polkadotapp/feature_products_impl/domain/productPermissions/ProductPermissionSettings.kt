package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionRecord
import javax.inject.Inject

sealed interface SavedProductPermission {
    val granted: Boolean

    data class Legacy(val status: ProductPermissionStatus) : SavedProductPermission {
        override val granted: Boolean get() = status.granted
    }

    data class TrUAPI(val record: PermissionRecord) : SavedProductPermission {
        override val granted: Boolean get() = record.status == PermissionAuthorizationStatus.AUTHORIZED
    }
}

class ProductPermissionSettings @Inject constructor(
    private val runtimeSettings: ProductRuntimeSettings,
    private val legacy: ProductPermissionRepository,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) {
    fun observePermissions(productId: ProductId): Flow<List<SavedProductPermission>> = flow {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            val runtime = runtimeProvider.constructedRuntime().getOrThrow()
            emitAll(runtime.observePermissionRecords(productId.value).map { records -> records.map { SavedProductPermission.TrUAPI(it) } })
        } else {
            emitAll(legacy.observeAllByProduct(productId).map { records -> records.map { SavedProductPermission.Legacy(it) } })
        }
    }

    suspend fun togglePermission(productId: ProductId, permission: SavedProductPermission) {
        when (permission) {
            is SavedProductPermission.TrUAPI -> {
                val status = if (permission.granted) PermissionAuthorizationStatus.DENIED else PermissionAuthorizationStatus.AUTHORIZED
                runtimeProvider.constructedRuntime().getOrThrow().setPermissionRecord(
                    permission.record.productId,
                    permission.record.request,
                    status
                )
            }
            is SavedProductPermission.Legacy -> {
                if (permission.granted) {
                    legacy.revoke(productId, permission.status.permission)
                } else {
                    legacy.grant(productId, permission.status.permission)
                }
            }
        }
    }
}
