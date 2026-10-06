package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import uniffi.truapi.AuthState
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ProductPermissionsInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val permissionRepository: ProductPermissionRepository,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) {
    private val automaticPermissionRevision = MutableStateFlow(0L)

    @OptIn(ExperimentalCoroutinesApi::class)
    fun observeAutomaticUploads(productId: ProductId): Flow<AutomaticPreimagePermission?> = flow {
        val runtime = runtimeProvider.runtime().getOrThrow()
        emitAll(
            combine(runtimeProvider.sessionState, automaticPermissionRevision) { session, _ -> session }
                .transformLatest { session ->
                    emit(null)
                    if (session !is AuthState.Connected) return@transformLatest
                    val root = runtime.currentSessionPublicKey() ?: return@transformLatest
                    if (!root.contentEquals(session.v1.publicKey)) return@transformLatest
                    val genesis = runtimeProvider.bulletinGenesisHash ?: return@transformLatest
                    try {
                        val status = runtime.permissionAuthorizationStatus(
                            productId.value, PermissionAuthorizationRequest.AutomaticPreimageSubmit(root),
                        )
                        if (isCurrentAccount(root) && root.contentEquals(runtime.currentSessionPublicKey())) {
                            emit(AutomaticPreimagePermission(root, genesis.toHexString(withPrefix = true), status))
                        }
                    } catch (error: CancellationException) {
                        throw error
                    } catch (_: Exception) {
                        // No stale or guessed grant is displayed when the read fails.
                        emit(null)
                    }
                },
        )
    }

    suspend fun setAutomaticUploads(
        productId: ProductId,
        rendered: AutomaticPreimagePermission,
        status: PermissionAuthorizationStatus,
    ) {
        check(isCurrentAccount(rendered.rootPublicKey)) { "Account changed; reopen app permissions" }
        val runtime = runtimeProvider.runtime().getOrThrow()
        check(isCurrentAccount(rendered.rootPublicKey) && rendered.rootPublicKey.contentEquals(runtime.currentSessionPublicKey())) {
            "Account changed; reopen app permissions"
        }
        try {
            runtime.setPermissionAuthorizationStatus(
                productId.value,
                PermissionAuthorizationRequest.AutomaticPreimageSubmit(rendered.rootPublicKey),
                status,
            )
        } finally {
            automaticPermissionRevision.update { it + 1 }
        }
    }

    private fun isCurrentAccount(root: ByteArray): Boolean =
        (runtimeProvider.sessionState.value as? AuthState.Connected)?.v1?.publicKey?.contentEquals(root) == true

    suspend fun getProduct(productId: ProductId): Product? {
        return productRepository.getProductById(productId)
    }

    fun observePermissions(productId: ProductId): Flow<List<ProductPermissionStatus>> {
        return permissionRepository.observeAllByProduct(productId)
    }

    suspend fun togglePermission(productId: ProductId, permissionStatus: ProductPermissionStatus) {
        if (permissionStatus.granted.not()) {
            permissionRepository.grant(productId, permissionStatus.permission)
        } else {
            permissionRepository.revoke(productId, permissionStatus.permission)
        }
    }
}

data class AutomaticPreimagePermission(
    val rootPublicKey: ByteArray,
    val genesisHash: String,
    val status: PermissionAuthorizationStatus,
)
