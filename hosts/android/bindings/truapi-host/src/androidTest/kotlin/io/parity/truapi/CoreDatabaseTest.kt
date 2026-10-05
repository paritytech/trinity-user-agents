package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.NativeRuntimeConfigException
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.RemotePermission

/**
 * The core database on a real device: the bundled SQLite in the shipped `.so`
 * opens a file in the app's no-backup directory, a misconfigured directory
 * stops the runtime from starting, and the durable engine runs over it.
 */
@RunWith(AndroidJUnit4::class)
class CoreDatabaseTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = context.noBackupFilesDir.resolve("truapi-test-${UUID.randomUUID()}")

    @After
    fun cleanUp() {
        directory.deleteRecursively()
    }

    @Test
    fun theCoreDatabaseOpensInTheConfiguredDirectory() {
        directory.mkdirs()
        val runtime = TrUAPIHostRuntime(InertBridge(), config(directory.absolutePath))

        val status = runtime.use { runBlocking { it.coreDatabaseStatus() } }

        val file = File(directory, "core.sqlite3")
        assertEquals(file.canonicalPath to 1u, status.path to status.schemaVersion)
        assertTrue("SQLite ${status.sqliteVersion} is not 3.x", status.sqliteVersion.startsWith("3."))
        assertTrue("database file was not created", file.exists())
    }

    /**
     * The durable engine starts with the database: the host hears whether
     * work is pending at launch, and recovery returns once nothing is live.
     */
    @Test
    fun durableWorkIsReportedAndRecoverySettlesOnAnEmptyLedger() {
        directory.mkdirs()
        val reported = LinkedBlockingQueue<Boolean>()
        val runtime = TrUAPIHostRuntime(InertBridge(onDurableWork = reported::put), config(directory.absolutePath))

        runtime.use {
            assertEquals(false, reported.poll(5, TimeUnit.SECONDS))
            runBlocking { it.runDurableRecovery() }
        }
    }

    @Test
    fun aMissingDirectoryStopsTheRuntimeFromStarting() {
        assertThrows(NativeRuntimeConfigException.DatabaseUnavailable::class.java) {
            TrUAPIHostRuntime(InertBridge(), config(directory.absolutePath))
        }
    }

    private fun config(databaseDirectory: String) = HostRuntimeConfig(
        hostName = "Core database test",
        peopleChainGenesisHash = ByteArray(32) { 0xa2.toByte() },
        bulletinChainGenesisHash = ByteArray(32) { 0xbb.toByte() },
        assetHubChainGenesisHash = ByteArray(32) { 0xcc.toByte() },
        networkSuffix = "paseo",
        databaseDirectory = databaseDirectory,
    )

    private inner class InertBridge(
        private val onDurableWork: (Boolean) -> Unit = {},
    ) : HostBridge {
        override val storage = PrefsHostStorage(
            context.getSharedPreferences("truapi_core_db_test_product", android.content.Context.MODE_PRIVATE),
        )
        override val coreStorage = PrefsHostCoreStorage(
            context.getSharedPreferences("truapi_core_db_test_core", android.content.Context.MODE_PRIVATE),
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
}
