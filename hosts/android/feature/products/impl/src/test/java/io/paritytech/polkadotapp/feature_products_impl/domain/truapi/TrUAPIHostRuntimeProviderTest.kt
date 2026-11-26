package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.pm.PackageInfo
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkObject
import io.mockk.slot
import io.mockk.unmockkObject
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.BundledStack
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.NativeTrUApiHostRuntime
import java.io.File
import java.util.Base64

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

    // The core derives the wallet's identities under the stack's TLD, so a build on another
    // network's stack mints keys for that network. Only the Polkadot build may use its reserved values.
    @Test
    fun `starts the core on the stack its chain environment bundles`() = runTest {
        val started = TestnetEnvironment.entries.associateWith { environment ->
            provider(environment = environment).runtime().getOrThrow()
            runtimeConfig.captured.stack.let { StartedStack(it.tld(), it.reservedValues.allowed) }
        }

        assertEquals(
            mapOf(
                TestnetEnvironment.TESTNET to StartedStack(tld = "testnet", allowsReservedValues = false),
                TestnetEnvironment.NIGHTLY to StartedStack(tld = "paseo", allowsReservedValues = false),
                TestnetEnvironment.PRODUCTION to StartedStack(tld = "dot", allowsReservedValues = true),
            ),
            started,
        )
    }

    private data class StartedStack(val tld: String, val allowsReservedValues: Boolean)

    private fun BundledStack.tld(): String {
        val envelope = Json.parseToJsonElement(descriptor.decodeToString()).jsonObject
        val payload = Base64.getDecoder().decode(envelope.getValue("payload").jsonPrimitive.content)
        return Json.parseToJsonElement(payload.decodeToString()).jsonObject.getValue("tld").jsonPrimitive.content
    }

    private fun TestScope.provider(
        environment: TestnetEnvironment = TestnetEnvironment.NIGHTLY,
    ) = TrUAPIHostRuntimeProvider(
        context = context(),
        environment = environment,
        chainDirectory = mockk(relaxed = true),
        localSessionSource = mockk { coEvery { resolve() } returns Result.failure(IllegalStateException()) },
        accountRepository = mockk { every { walletAccountFlow() } returns emptyFlow() },
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
        coreEventLogger = CoreEventLogger { _, _, _, _ -> },
        deviceKeyHandOver = mockk { every { keyToHandOver() } returns null },
        materialsSwitch = mockk(relaxed = true),
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
            // The stack assets the app ships, so a stack missing from them fails here rather than at launch.
            every { assets.open(any()) } answers {
                File(System.getProperty("truapi.stackAssets"), firstArg<String>()).inputStream()
            }
        }
    }
}
