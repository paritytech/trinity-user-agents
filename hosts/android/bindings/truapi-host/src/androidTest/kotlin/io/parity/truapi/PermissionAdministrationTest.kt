package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostRejection
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.PermissionAuthorizationEntry
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import uniffi.truapi.RemotePermission
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.io.path.createTempDirectory

@RunWith(AndroidJUnit4::class)
class PermissionAdministrationTest {
    @Test
    fun persistedGrantsSettingsRevocationAndFailureShareProcessAuthority() = runBlocking {
        val bridge = Bridge()
        val config = HostRuntimeConfig(
            hostName = "Permission administration test",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi-permissions").toString(),
        )
        val request = PermissionAuthorizationRequest.Device(HostDevicePermissionRequest.CAMERA)
        TrUAPIHostRuntime(bridge, config).use { runtime ->
            runtime.setPermissionAuthorizationStatus("game.paseo", request, PermissionAuthorizationStatus.AUTHORIZED)
        }
        // A fresh process runtime must enumerate existing backend keys, not just its new prompt history.
        TrUAPIHostRuntime(bridge, config).use { runtime ->
            assertEquals(listOf(PermissionAuthorizationEntry(request, PermissionAuthorizationStatus.AUTHORIZED)),
                runtime.permissionAuthorizations("game.paseo"))
            runtime.openProductExecution(bridge, ProductExecutionConfig("game.paseo", ProductExecutionKind.APP)).use { execution ->
                runtime.openProductExecution(bridge, ProductExecutionConfig("other.paseo", ProductExecutionKind.APP)).use { other ->
                    val revision = runtime.permissionAuthorizationRevision("game.paseo")
                    runtime.setPermissionAuthorizationStatus("game.paseo", request, PermissionAuthorizationStatus.DENIED)
                    assertTrue(execution.isClosed())
                    assertFalse(other.isClosed())
                    assertTrue(bridge.changed.contains("game.paseo"))
                    assertFalse(runtime.setPermissionAuthorizationStatusIfCurrent(
                        "game.paseo", request, PermissionAuthorizationStatus.AUTHORIZED, revision,
                    ))
                    assertEquals(listOf(PermissionAuthorizationEntry(request, PermissionAuthorizationStatus.DENIED)),
                        runtime.importPermissionAuthorizations("game.paseo", listOf(
                            PermissionAuthorizationEntry(request, PermissionAuthorizationStatus.AUTHORIZED),
                        )))
                }
            }
            runtime.setPermissionAuthorizationStatus("game.paseo", request, PermissionAuthorizationStatus.NOT_DETERMINED)
            runtime.openProductExecution(bridge, ProductExecutionConfig("game.paseo", ProductExecutionKind.APP)).use { reopened ->
                assertFalse(reopened.isClosed())
                assertEquals(PermissionAuthorizationStatus.NOT_DETERMINED, runtime.permissionAuthorizations("game.paseo").single().status)
                runtime.setPermissionAuthorizationStatus("game.paseo", request, PermissionAuthorizationStatus.DENIED)
                assertTrue(reopened.isClosed())
            }
            bridge.store.failWrites = true
            assertTrue(runCatching {
                runtime.setPermissionAuthorizationStatus("game.paseo", request, PermissionAuthorizationStatus.AUTHORIZED)
            }.isFailure)
            assertEquals(PermissionAuthorizationStatus.DENIED, runtime.permissionAuthorizations("game.paseo").single().status)
        }
    }

    private class Bridge : HostBridge {
        val changed = CopyOnWriteArrayList<String>()
        val store = Storage()
        override val storage: HostStorage = store
        override val coreStorage: HostCoreStorage = store
        override fun permissionAuthorizationsChanged(productId: String) { changed.add(productId) }
        override suspend fun navigateTo(url: String) = Unit
        override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false
        override suspend fun devicePermission(product: ProductExecutionConfig, request: HostDevicePermissionRequest) = PermissionDecision.DENY
        override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission) = PermissionDecision.DENY
    }

    private class Storage : HostCoreStorage, HostStorage {
        private val core = ConcurrentHashMap<List<Byte>, ByteArray>()
        private val product = ConcurrentHashMap<String, ByteArray>()
        @Volatile var failWrites = false
        override suspend fun keys(): List<ByteArray> = core.keys.map { it.toByteArray() }
        override suspend fun read(key: ByteArray): ByteArray? = core[key.toList()]
        override suspend fun write(key: ByteArray, value: ByteArray) {
            if (failWrites) throw HostRejection.Rejected("disk full")
            core[key.toList()] = value
        }
        override suspend fun clear(key: ByteArray) { core.remove(key.toList()) }
        override suspend fun read(key: String): ByteArray? = product[key]
        override suspend fun write(key: String, value: ByteArray) { product[key] = value }
        override suspend fun clear(key: String) { product.remove(key) }
    }
}
