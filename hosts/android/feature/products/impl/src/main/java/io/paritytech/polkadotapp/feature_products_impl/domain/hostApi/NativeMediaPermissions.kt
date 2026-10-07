package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.Manifest
import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.EncryptedHostCoreStorage
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia.NativeMediaConsent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import javax.inject.Inject
import javax.inject.Singleton

data class NativeMediaPermissionStatus(
    val request: PermissionAuthorizationRequest,
    val granted: Boolean,
    val ownerAccountId: Long,
    val productId: String,
    val runtimeGeneration: Long,
)

/** Trusted settings for the same process runtime and physical policy slots as live products. */
@Singleton
class NativeMediaPermissions @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val accounts: AccountRepository,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val bridgeFactory: ProductTrUAPIHostBridge.Factory,
) {
    fun observeMediaPermissions(product: ProductId): Flow<List<NativeMediaPermissionStatus>> = flow {
        runtimeProvider.runtime().getOrThrow()
        emitAll(combine(EncryptedHostCoreStorage.storageChanges, runtimeProvider.sessionRevision) { _, _ ->
            withPermissionAdmin(product) { execution ->
                runtimeProvider.withSessionMutation {
                    val owner = accounts.getWalletAccount().id
                    check(runtimeProvider.sessionOwner == owner) { "Wallet session changed" }
                    val requests = listOf(
                        PermissionAuthorizationRequest.Device(HostDevicePermissionRequest.CAMERA),
                        PermissionAuthorizationRequest.Device(HostDevicePermissionRequest.MICROPHONE),
                        execution.callingPermissionAuthorizationRequest(),
                    )
                    requests.map { request ->
                        NativeMediaPermissionStatus(request,
                            execution.permissionAuthorizationStatus(request) == PermissionAuthorizationStatus.AUTHORIZED,
                            owner, product.value, runtimeProvider.sessionRevision.value)
                    }
                }
            }
        })
    }

    suspend fun toggleMediaPermission(product: ProductId, permission: NativeMediaPermissionStatus) {
        withPermissionAdmin(product) { execution ->
            runtimeProvider.withSessionMutation { checkScope(product, permission, execution) }
            val request = permission.request
            val granted = if (permission.granted) false else when (request) {
                is PermissionAuthorizationRequest.Calling ->
                    NativeMediaConsent.calling(context, product.value, request.network, request.account)
                is PermissionAuthorizationRequest.Device -> NativeMediaConsent.device(context, product.value,
                    when (request.v1) {
                        HostDevicePermissionRequest.CAMERA -> Manifest.permission.CAMERA
                        HostDevicePermissionRequest.MICROPHONE -> Manifest.permission.RECORD_AUDIO
                        else -> error("Not a Media device permission")
                    })
                else -> error("Not a Media permission")
            }
            runtimeProvider.withSessionMutation {
                checkScope(product, permission, execution)
                withContext(Dispatchers.IO) {
                    execution.setPermissionAuthorizationStatus(request,
                        if (granted) PermissionAuthorizationStatus.AUTHORIZED else PermissionAuthorizationStatus.DENIED)
                }
            }
            // Commit and enqueue happen atomically in the SDK. Never await a refresh under
            // the session mutation gate: refresh may need that same runtime to make progress.
            execution.awaitCoreStorageChanges()
        }
    }

    private suspend fun checkScope(product: ProductId, permission: NativeMediaPermissionStatus, execution: TrUAPIProductExecution) {
        check(product.value == permission.productId &&
            runtimeProvider.sessionOwner == permission.ownerAccountId &&
            accounts.getWalletAccount().id == permission.ownerAccountId &&
            runtimeProvider.sessionRevision.value == permission.runtimeGeneration) { "Wallet session changed" }
        if (permission.request is PermissionAuthorizationRequest.Calling) {
            val current = execution.callingPermissionAuthorizationRequest() as? PermissionAuthorizationRequest.Calling
            check(current != null && current.network.contentEquals(permission.request.network) &&
                current.account.contentEquals(permission.request.account)) { "Calling identity changed" }
        }
    }

    private suspend fun <T> withPermissionAdmin(product: ProductId, block: suspend (TrUAPIProductExecution) -> T): T = coroutineScope {
        val runtime = runtimeProvider.runtime().getOrThrow()
        val bridge = bridgeFactory.create(this)
        var opened: TrUAPIProductExecution? = null
        try {
            withContext(Dispatchers.IO) { opened = bridge.openPermissionExecution(runtime, product) }
            block(requireNotNull(opened))
        } finally {
            opened?.close()
        }
    }
}
