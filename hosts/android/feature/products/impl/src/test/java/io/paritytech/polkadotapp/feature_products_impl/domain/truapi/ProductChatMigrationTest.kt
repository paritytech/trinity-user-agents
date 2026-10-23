package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.SharedPreferences
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatPaymentAdoption
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class ProductChatMigrationTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val history = ByteArray(10) { it.toByte() }
    private val secret = ByteArray(32) { 0x5a }
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private val stored = mutableMapOf<String, Boolean>()
    private val preferences = mockk<SharedPreferences> {
        every { getBoolean(any(), any()) } answers { stored[firstArg()] ?: secondArg() }
        every { edit() } returns mockk {
            every { putBoolean(any(), any()) } answers { stored[firstArg()] = secondArg(); self as SharedPreferences.Editor }
            every { apply() } returns Unit
        }
    }
    private val exporter = mockk<NativeChatHistoryExporter> { coEvery { export() } returns history }
    private val runtime = mockk<TrUAPIHostRuntime> { coEvery { importChatDeviceKey(any()) } returns true }
    private val deviceKeys = mockk<OurDeviceKeypairProvider> {
        coEvery { get() } returns X25519KeyPair(
            X25519PrivateKey.fromDerivedBytes(secret),
            X25519PublicKey.fromBytes(DataByteArray(ByteArray(32))).getOrThrow(),
        )
    }

    /** The room each adoption gave for bob. */
    private val roomsOfBob = mutableListOf<ChatId>()
    private var adoptionFails = false
    private val payments = mockk<NativeChatPaymentAdoption> {
        coEvery { adopt(any()) } answers {
            if (adoptionFails) error("the database is not there")
            roomsOfBob += firstArg<(AccountId) -> ChatId>()(bob)
            2
        }
    }
    private val tldProvider = mockk<DotNsTldProvider> {
        coEvery { getTld() } returns Result.success(requireNotNull(DotNsTld.parse("paseo")))
    }

    private fun migration() = ProductChatMigration(
        exporter = exporter,
        payments = payments,
        tldProvider = tldProvider,
        deviceKeys = deviceKeys,
        runtimeProvider = { mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) } },
        preferences = preferences,
        file = File(folder.root, "history.bin"),
    )

    private fun TestScope.launched() = migration().also {
        with(ComputationalScope(backgroundScope)) { it.initialize() }
        runCurrent()
    }

    @Test
    fun `the native history is staged once, read in parts, and the native device key goes to the core`() = runTest {
        val migration = launched()

        val first = migration.part(0u, 4u)
        val rest = migration.part(4u, 100u)

        assertEquals(10uL, first.total)
        assertArrayEquals(history, first.data + rest.data)
        coVerify(exactly = 1) { runtime.importChatDeviceKey(secret) }

        migration.imported()
        assertEquals(0uL, migration.part(0u, 4u).total)

        // A later launch does not stage the history again.
        launched().part(0u, 4u)
        coVerify(exactly = 1) { exporter.export() }
    }

    // Every install that ran the native chat moves to the chat product; there is no other chat to stay in.
    @Test
    fun `the native payments go to the contacts' rooms of the chat product once`() = runTest {
        launched()
        launched()

        coVerify(exactly = 1) { payments.adopt(any()) }
        assertEquals(listOf(ChatId.forExtensionRoom("ProductBot_chat.paseo", "dm-" + "b0".repeat(32))), roomsOfBob)
    }

    /** The payments step came after some installs had staged their history already; those take their payments too. */
    @Test
    fun `an install that staged its history before takes its payments over too`() = runTest {
        stored["staged"] = true

        launched()

        coVerify(exactly = 0) { exporter.export() }
        coVerify(exactly = 1) { payments.adopt(any()) }
        assertTrue(stored["payments_adopted"] == true)
    }

    @Test
    fun `payments that could not be taken over are taken at the next launch`() = runTest {
        adoptionFails = true
        launched()
        assertFalse(stored["payments_adopted"] == true)

        adoptionFails = false
        launched()

        coVerify(exactly = 2) { payments.adopt(any()) }
        assertTrue(stored["payments_adopted"] == true)
    }
}
