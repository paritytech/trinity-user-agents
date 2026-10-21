package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.SharedPreferences
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class ProductChatMigrationTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val history = ByteArray(10) { it.toByte() }
    private val secret = ByteArray(32) { 0x5a }
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

    private fun migration(productChat: Boolean) = ProductChatMigration(
        mode = mockk<PersonChatMode> { every { isProductChat() } returns productChat },
        exporter = exporter,
        deviceKeys = deviceKeys,
        runtimeProvider = { mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) } },
        preferences = preferences,
        file = File(folder.root, "history.bin"),
    )

    private fun TestScope.launched(productChat: Boolean) = migration(productChat).also {
        with(ComputationalScope(backgroundScope)) { it.initialize() }
    }

    @Test
    fun `the native history is staged once, read in parts, and the native device key goes to the core`() = runTest {
        val migration = launched(productChat = true)

        val first = migration.part(0u, 4u)
        val rest = migration.part(4u, 100u)

        assertEquals(10uL, first.total)
        assertArrayEquals(history, first.data + rest.data)
        coVerify(exactly = 1) { runtime.importChatDeviceKey(secret) }

        migration.imported()
        assertEquals(0uL, migration.part(0u, 4u).total)

        // A later launch does not stage the history again.
        launched(productChat = true).part(0u, 4u)
        coVerify(exactly = 1) { exporter.export() }
    }

    @Test
    fun `an install that runs the native chat stages nothing`() = runTest {
        assertEquals(0uL, launched(productChat = false).part(0u, 4u).total)
        coVerify(exactly = 0) { exporter.export() }
        coVerify(exactly = 0) { runtime.importChatDeviceKey(any()) }
    }
}
