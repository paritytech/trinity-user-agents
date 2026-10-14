package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.VerifySignature
import io.novasama.substrate_sdk_android.runtime.metadata.TransactionExtensionMetadata
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeProvider
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealDefaultExtrinsicVersionProviderTest {
    private val remoteConfig = mockk<RemoteConfigService>()
    private val chainRegistry = mockk<ChainRegistry>()

    private val provider = RealDefaultExtrinsicVersionProvider(
        knownChains = KnownChains(people = PEOPLE, assetHub = ASSET_HUB, bulletIn = "bullet-in", hydration = null),
        remoteConfigService = remoteConfig,
        chainRegistry = chainRegistry,
    )

    @Before
    fun setUp() {
        coEvery { remoteConfig.getSyncedJsonObject(any(), Map::class.java) } returns
            Result.success(mapOf(PEOPLE to 1.0, ASSET_HUB to 1.0))
    }

    @Test
    fun `a signed asset hub transaction goes general when the runtime verifies signatures in that version`() = runTest {
        runtimeOf(ASSET_HUB, extensionVersion = 1, VerifySignature.ID, "ChargeAssetTxPayment")

        assertGeneral(extensionVersion = 1, provider.getDefaultExtrinsicVersion(ASSET_HUB, isSigned = true).getOrThrow())
    }

    @Test
    fun `a signed asset hub transaction stays v4 when the runtime cannot verify a general signature`() = runTest {
        runtimeOf(ASSET_HUB, extensionVersion = 1, "AsPgas", "ChargeAssetTxPayment")

        assertEquals(ExtrinsicVersion.V4, provider.getDefaultExtrinsicVersion(ASSET_HUB, isSigned = true).getOrThrow())
    }

    @Test
    fun `a signed people transaction stays v4 when the runtime cannot verify a general signature`() = runTest {
        runtimeOf(PEOPLE, extensionVersion = 1, "AsPerson", "ChargeAssetTxPayment")

        assertEquals(ExtrinsicVersion.V4, provider.getDefaultExtrinsicVersion(PEOPLE, isSigned = true).getOrThrow())
    }

    @Test
    fun `a signed people transaction goes general when the runtime verifies signatures in that version`() = runTest {
        runtimeOf(PEOPLE, extensionVersion = 1, VerifySignature.ID, "AsPerson")

        assertGeneral(extensionVersion = 1, provider.getDefaultExtrinsicVersion(PEOPLE, isSigned = true).getOrThrow())
    }

    @Test
    fun `an unsigned transaction goes general whatever the runtime verifies`() = runTest {
        runtimeOf(ASSET_HUB, extensionVersion = 1, "AsPgas")

        assertGeneral(extensionVersion = 1, provider.getDefaultExtrinsicVersion(ASSET_HUB, isSigned = false).getOrThrow())
    }

    @Test
    fun `other chains stay v4`() = runTest {
        assertEquals(ExtrinsicVersion.V4, provider.getDefaultExtrinsicVersion("polkadot", isSigned = true).getOrThrow())
    }

    private fun runtimeOf(chainId: ChainId, extensionVersion: Int, vararg extensionIds: String) {
        val extensions = extensionIds.map { extensionId -> mockk<TransactionExtensionMetadata> { every { id } returns extensionId } }
        val runtime = mockk<RuntimeSnapshot> {
            every { metadata.extrinsic.transactionExtensions(extensionVersion) } returns extensions
        }

        coEvery { chainRegistry.getRuntimeProvider(chainId) } returns mockk<RuntimeProvider> { coEvery { get() } returns runtime }
    }

    private fun assertGeneral(extensionVersion: Byte, actual: ExtrinsicVersion) {
        assertTrue("expected a general transaction, got $actual", actual is ExtrinsicVersion.V5)
        assertEquals(extensionVersion, (actual as ExtrinsicVersion.V5).extensionVersion)
    }

    private companion object {
        const val PEOPLE = "people"
        const val ASSET_HUB = "asset-hub"
    }
}
