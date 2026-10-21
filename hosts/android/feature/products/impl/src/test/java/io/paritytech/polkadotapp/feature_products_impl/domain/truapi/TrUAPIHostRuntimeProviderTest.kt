package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.pm.PackageInfo
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkObject
import io.mockk.slot
import io.mockk.unmockkObject
import io.mockk.verify
import io.mockk.verifyOrder
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
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

    // The core holds a foreground worker only while the host reports the app in the foreground, and it starts in the
    // background, so the host reports the state at the build and at each change.
    @Test
    fun `the core learns the foreground state at the build and at each change`() = runTest {
        val native = mockk<NativeTrUApiHostRuntime>(relaxed = true)
        every { NativeTrUApiHostRuntime.withRuntimeConfig(any(), capture(runtimeConfig)) } returns native
        val lifecycle = MutableStateFlow(AppLifecycleState.FOREGROUND)
        val supervisor = mockk<TrUAPIWorkerSupervisor>(relaxed = true)

        provider(lifecycle, supervisor).runtime().getOrThrow()
        runCurrent()
        lifecycle.value = AppLifecycleState.BACKGROUND
        runCurrent()
        lifecycle.value = AppLifecycleState.FOREGROUND
        runCurrent()

        verifyOrder {
            native.setAppForeground(true)
            native.setAppForeground(false)
            native.setAppForeground(true)
        }
        // A worker that ended on a fault starts again at each return to the foreground.
        verify(exactly = 2) { supervisor.restartFailed() }
    }

    private fun TestScope.provider(
        lifecycle: Flow<AppLifecycleState> = emptyFlow(),
        supervisor: TrUAPIWorkerSupervisor = mockk(relaxed = true),
    ) = TrUAPIHostRuntimeProvider(
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
        appLifecycleObserver = mockk { every { subscribe() } returns lifecycle },
        contactsBridge = mockk { every { contactRemovals() } returns emptyFlow() },
        paymentPlatforms = mockk(relaxed = true),
        scannerBridge = mockk(),
        personChatBridge = mockk(),
        pushBridge = mockk(),
        callMediaBridge = mockk(),
        workerSupervisor = { supervisor },
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
