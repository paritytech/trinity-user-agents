package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class AppPersonChatHostBridgeTest {
    private fun bridgeFor(hopUrls: List<String>?, nodeUrl: String): AppPersonChatHostBridge {
        val node = mockk<Chain.Node> { every { unformattedUrl } returns nodeUrl }
        val chain = mockk<Chain> {
            every { externalApis } returns listOfNotNull(hopUrls?.let { Chain.ExternalApi.Hop(it) })
            every { nodes } returns mockk { every { nodes } returns listOf(node) }
        }
        val registry = mockk<ChainRegistry> { coEvery { bulletInChain() } returns chain }
        return AppPersonChatHostBridge(registry, mockk(), mockk(), mockk())
    }

    @Test
    fun `the product gets the HOP nodes the native chat uploads to`() = runTest {
        val bridge = bridgeFor(listOf("wss://hop-1.example", "wss://hop-2.example"), "wss://bulletin.example")

        assertEquals(listOf("wss://hop-1.example", "wss://hop-2.example"), bridge.hopNodes())
    }

    @Test
    fun `without HOP nodes in the registry the Bulletin node is used, as the native chat does`() = runTest {
        val bridge = bridgeFor(null, "wss://bulletin.example")

        assertEquals(listOf("wss://bulletin.example"), bridge.hopNodes())
    }

    @Test
    fun `paired devices are the active SSO sessions, keyed as the native chat announces them`() = runTest {
        val session = ActiveSsoSession(
            id = "s1",
            statementAccountId = ByteArray(32) { 1 }.toDataByteArray(),
            encryptionPublicKey = X25519PublicKey.fromBytes(ByteArray(32) { 2 }.toDataByteArray()).getOrThrow(),
            name = "Desktop",
            icon = "",
            hostVersion = null,
            platformType = null,
            platformVersion = null,
            addedAt = 0,
            status = DeviceStatus.ACTIVE,
            lastUpdate = 0,
        )
        val sessions = mockk<GetActiveSsoSessionsUseCase> { coEvery { getSessions() } returns listOf(session) }
        val bridge = AppPersonChatHostBridge(mockk(), sessions, mockk(), mockk())

        val device = bridge.pairedDevices().single()
        assertArrayEquals(ByteArray(32) { 1 }, device.statementAccountId)
        assertArrayEquals(ByteArray(32) { 2 }, device.encryptionPublicKey)
        assertEquals("Desktop", device.name)
    }
}
