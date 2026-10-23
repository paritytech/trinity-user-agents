package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatPayments
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.HostChatPaymentReceipt
import uniffi.truapi.HostPersonChatPaymentReceivedRequest
import java.math.BigInteger

class AppPersonChatHostBridgeTest {
    private fun bridgeFor(hopUrls: List<String>?, nodeUrl: String): AppPersonChatHostBridge {
        val node = mockk<Chain.Node> { every { unformattedUrl } returns nodeUrl }
        val chain = mockk<Chain> {
            every { externalApis } returns listOfNotNull(hopUrls?.let { Chain.ExternalApi.Hop(it) })
            every { nodes } returns mockk { every { nodes } returns listOf(node) }
        }
        val registry = mockk<ChainRegistry> { coEvery { bulletInChain() } returns chain }
        return AppPersonChatHostBridge(registry, mockk(), mockk(), mockk(), mockk())
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
        val bridge = AppPersonChatHostBridge(mockk(), sessions, mockk(), mockk(), mockk())

        val device = bridge.pairedDevices().single()
        assertArrayEquals(ByteArray(32) { 1 }, device.statementAccountId)
        assertArrayEquals(ByteArray(32) { 2 }, device.encryptionPublicKey)
        assertEquals("Desktop", device.name)
    }

    // The product asks again until the host takes a payment, so the host keeps it by message id and accepts it, also
    // before its coinage flow runs.
    @Test
    fun `a payment of a contact is kept for the native claim under its message id and accepted`() = runTest {
        val payments = RecordingPayments()
        val bridge = AppPersonChatHostBridge(mockk(), mockk(), mockk(), mockk(), payments)
        val keys = listOf(ByteArray(64) { 7 })

        val receipt = bridge.paymentReceived(
            "chat.paseo",
            HostPersonChatPaymentReceivedRequest(roomId = "dm-ab", messageId = "m-1", timestamp = 1_000uL, totalValue = "1500", coinKeys = keys),
        )

        assertEquals(HostChatPaymentReceipt.ACCEPTED, receipt)
        val received = payments.received.single()
        assertEquals(ProductChatIdParameter("dm-ab").toChatId(ProductId.fromStoredValue("chat.paseo").toChatExtensionId()), received.chatId)
        assertEquals("m-1", received.messageId)
        assertEquals(1_000L, received.timestamp)
        assertEquals(BigInteger.valueOf(1500), received.totalValue.value)
        assertArrayEquals(keys.single(), received.coinKeys.single())
    }

    private class RecordingPayments : ProductChatPayments {
        class Received(val chatId: ChatId, val messageId: String, val timestamp: Long, val totalValue: Balance, val coinKeys: List<ByteArray>)

        val received = mutableListOf<Received>()

        override suspend fun send(recipient: AccountId, totalValue: Balance, coinKeys: List<ByteArray>, onSaved: suspend () -> Unit) = Unit

        override suspend fun receive(chatId: ChatId, messageId: String, timestamp: Long, totalValue: Balance, coinKeys: List<ByteArray>) {
            received += Received(chatId, messageId, timestamp, totalValue, coinKeys)
        }
    }
}
