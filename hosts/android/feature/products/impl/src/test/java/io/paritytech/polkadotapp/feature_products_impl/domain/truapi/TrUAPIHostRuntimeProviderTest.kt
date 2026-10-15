package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.pm.PackageInfo
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkObject
import io.mockk.slot
import io.mockk.unmockkObject
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.HostCallbacks
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostRejection
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.NativeTrUApiHostRuntime

class TrUAPIHostRuntimeProviderTest {
    @get:Rule
    val tempFolder = TemporaryFolder()

    private val runtimeConfig = slot<HostRuntimeConfig>()

    @Before
    fun captureRuntimeConfig() {
        mockkObject(NativeTrUApiHostRuntime.Companion)
        every { NativeTrUApiHostRuntime.withRuntimeConfig(any(), capture(runtimeConfig)) } returns mockk(relaxed = true)
    }

    @After
    fun restoreRuntimeFactory() = unmockkObject(NativeTrUApiHostRuntime.Companion)

    @Test
    fun `reports the installed version and build so products can tell host builds apart`() = runTest {
        provider().runtime().getOrThrow()

        assertEquals("1.0.0 (1022)", runtimeConfig.captured.hostVersion)
    }

    @Test
    fun `a permission ask with no product screen ends with an error and not with a refusal`() = runTest {
        val callbacks = slot<HostCallbacks>()
        every {
            NativeTrUApiHostRuntime.withRuntimeConfig(capture(callbacks), capture(runtimeConfig))
        } returns mockk(relaxed = true)
        provider().runtime().getOrThrow()

        val device = runCatching {
            callbacks.captured.devicePermission(mockk(relaxed = true), HostDevicePermissionRequest.entries.first())
        }
        val remote = runCatching { callbacks.captured.remotePermission(mockk(relaxed = true), mockk(relaxed = true)) }

        assertTrue(device.exceptionOrNull() is HostRejection)
        assertTrue(remote.exceptionOrNull() is HostRejection)
    }

    private fun TestScope.provider() = TrUAPIHostRuntimeProvider(
        context = context(),
        chainRegistry = mockk(relaxed = true),
        knownChains = KnownChains(people = "people", assetHub = "asset-hub", bulletIn = "bullet-in", hydration = null),
        chainDirectory = mockk(relaxed = true),
        localSessionSource = mockk { coEvery { resolve() } returns Result.failure(IllegalStateException()) },
        accountRepository = mockk { every { walletAccountFlow() } returns emptyFlow() },
        dotNsTldProvider = mockk { coEvery { getTld() } returns Result.success(DotNsTld.parse("paseo")!!) },
        encryptedPreferences = mockk(relaxed = true),
        chainHttpClient = mockk(relaxed = true),
        confirmationLauncher = mockk(relaxed = true),
        appLifecycleObserver = mockk { every { subscribe() } returns emptyFlow() },
        contactsBridge = mockk { every { contactRemovals() } returns emptyFlow() },
        scannerBridge = mockk(),
        workerSupervisor = { mockk(relaxed = true) },
        dispatchers = testDispatchers(),
    )

    private fun context(): Context {
        val packageInfo = mockk<PackageInfo> { every { longVersionCode } returns 1022L }
        packageInfo.versionName = "1.0.0"
        return mockk {
            every { packageName } returns "io.parity.polkadotapp"
            every { packageManager.getPackageInfo("io.parity.polkadotapp", 0) } returns packageInfo
            every { noBackupFilesDir } returns tempFolder.root
        }
    }
}
