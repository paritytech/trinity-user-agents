package io.paritytech.polkadotapp.feature_transactions_impl.data.extensions

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.VerifySignature
import io.novasama.substrate_sdk_android.runtime.metadata.TransactionExtensionMetadata
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.RuntimeProvider
import io.paritytech.polkadotapp.feature_transactions_impl.data.RealDefaultExtrinsicVersionProvider
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealTxPayloadExtensionsResolverTest {
    private val remoteConfig = mockk<RemoteConfigService>()
    private val chainRegistry = mockk<ChainRegistry>()

    private val resolver = RealTxPayloadExtensionsResolver(
        defaultExtrinsicVersionProvider = RealDefaultExtrinsicVersionProvider(
            knownChains = KnownChains(people = PEOPLE, assetHub = ASSET_HUB, bulletIn = "bullet-in", hydration = null),
            remoteConfigService = remoteConfig,
            chainRegistry = chainRegistry,
        ),
        chainRegistry = chainRegistry,
    )

    @Before
    fun setUp() {
        coEvery { remoteConfig.getSyncedJsonObject(any(), Map::class.java) } returns Result.success(mapOf(ASSET_HUB to 1.0))
    }

    @Test
    fun `a signed request on extension version 0 builds v4 when that version cannot verify a general signature`() = runTest {
        runtimeOf(
            ASSET_HUB,
            0 to listOf("AsPgas", "ChargeAssetTxPayment"),
            1 to listOf(VerifySignature.ID, "AsPgas", "ChargeAssetTxPayment"),
        )

        assertEquals(ExtrinsicVersion.V4, resolveVersion(isSigned = true))
    }

    @Test
    fun `a signed request on extension version 0 goes general when that version verifies signatures`() = runTest {
        runtimeOf(
            ASSET_HUB,
            0 to listOf(VerifySignature.ID, "ChargeAssetTxPayment"),
            1 to listOf(VerifySignature.ID, "AsPgas", "ChargeAssetTxPayment"),
        )

        assertGeneral(extensionVersion = 0, resolveVersion(isSigned = true))
    }

    @Test
    fun `an unsigned request on extension version 0 goes general whatever that version verifies`() = runTest {
        runtimeOf(
            ASSET_HUB,
            0 to listOf("AsPgas", "ChargeAssetTxPayment"),
            1 to listOf(VerifySignature.ID, "AsPgas", "ChargeAssetTxPayment"),
        )

        assertGeneral(extensionVersion = 0, resolveVersion(isSigned = false))
    }

    private suspend fun resolveVersion(isSigned: Boolean): ExtrinsicVersion? {
        return resolver.resolve(extensions = emptyList(), txExtVersion = 0u, chainId = ASSET_HUB, isSigned = isSigned)
            .getOrThrow()
            .submissionOptions
            .extrinsicVersion
    }

    private fun runtimeOf(chainId: String, vararg extensionIdsByVersion: Pair<Int, List<String>>) {
        val runtime = mockk<RuntimeSnapshot> {
            extensionIdsByVersion.forEach { (extensionVersion, extensionIds) ->
                val extensions = extensionIds.map { extensionId -> mockk<TransactionExtensionMetadata> { every { id } returns extensionId } }
                every { metadata.extrinsic.transactionExtensions(extensionVersion) } returns extensions
            }
        }

        coEvery { chainRegistry.getRuntimeProvider(chainId) } returns mockk<RuntimeProvider> { coEvery { get() } returns runtime }
    }

    private fun assertGeneral(extensionVersion: Byte, actual: ExtrinsicVersion?) {
        assertTrue("expected a general transaction, got $actual", actual is ExtrinsicVersion.V5)
        assertEquals(extensionVersion, (actual as ExtrinsicVersion.V5).extensionVersion)
    }

    private companion object {
        const val PEOPLE = "people"
        const val ASSET_HUB = "asset-hub"
    }
}
