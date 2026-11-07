package io.parity.truapi

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.truapi.BundledStack
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
    stack = previewnetStack(),
    databaseDirectory = databaseDirectory,
)

/** Previewnet's signed descriptor and link, packaged into the test APK from the core's copies. */
fun previewnetStack(): BundledStack {
    val assets = InstrumentationRegistry.getInstrumentation().context.assets
    fun asset(name: String) = assets.open(name).use { it.readBytes() }
    return BundledStack(
        pointer = asset("previewnet.link").decodeToString().trim(),
        descriptor = asset("previewnet.json"),
        allowsReservedValues = false,
    )
}
