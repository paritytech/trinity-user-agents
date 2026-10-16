package io.parity.truapi

import android.content.Context
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.RemotePermission

/** A bridge that refuses everything a product could ask for, for runtime-level tests. */
class InertHostBridge(
    context: Context,
    private val onDurableWork: (Boolean) -> Unit = {},
) : HostBridge {
    override val storage = PrefsHostStorage(
        context.getSharedPreferences("truapi_inert_bridge_product", Context.MODE_PRIVATE),
    )
    override val coreStorage = PrefsHostCoreStorage(
        context.getSharedPreferences("truapi_inert_bridge_core", Context.MODE_PRIVATE),
    )

    override suspend fun navigateTo(url: String) = Unit

    override suspend fun devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest,
    ) = PermissionDecision.DENY

    override suspend fun remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission,
    ) = PermissionDecision.DENY

    override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false

    override fun durableWorkChanged(pending: Boolean) = onDurableWork(pending)
}

/** A runtime config whose core database lives in [databaseDirectory]. */
fun instrumentedRuntimeConfig(databaseDirectory: String) = HostRuntimeConfig(
    hostName = "Instrumented test host",
    peopleChainGenesisHash = ByteArray(32) { 0xa2.toByte() },
    bulletinChainGenesisHash = ByteArray(32) { 0xbb.toByte() },
    assetHubChainGenesisHash = ByteArray(32) { 0xcc.toByte() },
    networkSuffix = "paseo",
    databaseDirectory = databaseDirectory,
)
