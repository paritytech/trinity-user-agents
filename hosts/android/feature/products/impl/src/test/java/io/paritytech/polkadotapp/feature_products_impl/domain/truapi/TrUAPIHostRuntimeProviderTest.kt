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
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.FakePinnedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.personhoodProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.pinnedCard
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.NativeTrUApiHostRuntime
import uniffi.truapi.PinnedPocketCard

class TrUAPIHostRuntimeProviderTest {
    @get:Rule
    val tempFolder = TemporaryFolder()

    private val runtimeConfig = slot<HostRuntimeConfig>()
    private val paseo = requireNotNull(DotNsTld.parse("paseo"))

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

    // The Pocket is read from the runtime, so a runtime that waits on the chain for the network's
    // name shows no cards offline. The name an earlier run read is the one this build's network has.
    @Test
    fun `boots offline on the network name an earlier run saved`() = runTest {
        provider(saved = paseo, read = Result.failure(IllegalStateException("offline"))).runtime().getOrThrow()

        assertEquals("paseo", runtimeConfig.captured.networkSuffix)
    }

    @Test
    fun `starts the core with the cards the host pins`() = runTest {
        provider().runtime().getOrThrow()

        assertEquals(
            listOf(PinnedPocketCard(personhoodProduct.value, "humanity", "humanity")),
            runtimeConfig.captured.pinnedPocketCards,
        )
    }

    private fun TestScope.provider(
        saved: DotNsTld? = null,
        read: Result<DotNsTld> = Result.success(paseo),
    ) = TrUAPIHostRuntimeProvider(
        context = context(),
        chainRegistry = mockk(relaxed = true),
        knownChains = KnownChains(people = "people", assetHub = "asset-hub", bulletIn = "bullet-in", hydration = null),
        chainDirectory = mockk(relaxed = true),
        localSessionSource = mockk { coEvery { resolve() } returns Result.failure(IllegalStateException()) },
        accountRepository = mockk { every { walletAccountFlow() } returns emptyFlow() },
        dotNsTldProvider = mockk {
            every { currentTldOrNull() } returns saved
            coEvery { getTld() } returns read
        },
        encryptedPreferences = mockk(relaxed = true),
        chainHttpClient = mockk(relaxed = true),
        confirmationLauncher = mockk(relaxed = true),
        appLifecycleObserver = mockk { every { subscribe() } returns emptyFlow() },
        networkStateService = mockk { every { defaultNetwork } returns MutableStateFlow(null) },
        contactsBridge = mockk { every { contactRemovals() } returns emptyFlow() },
        paymentPlatforms = mockk(relaxed = true),
        scannerBridge = mockk(),
        installations = mockk(),
        extensionVersions = mockk(),
        pinnedPocketCards = FakePinnedPocketCards(listOf(pinnedCard(personhoodProduct, "humanity"))),
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
