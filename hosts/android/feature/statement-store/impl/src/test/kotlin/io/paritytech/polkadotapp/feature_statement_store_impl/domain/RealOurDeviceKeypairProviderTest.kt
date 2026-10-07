package io.paritytech.polkadotapp.feature_statement_store_impl.domain

import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.spyk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.utils.X25519KeyGenerator
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class RealOurDeviceKeypairProviderTest {
    private val preferences = mockk<EncryptedPreferences>()
    private val keyGenerator = spyk(X25519KeyGenerator())
    private val provider = RealOurDeviceKeypairProvider(preferences, keyGenerator)

    @Test
    fun `unreadable device storage cannot generate a replacement identity`() = runTest {
        val failure = IllegalStateException("device storage is inaccessible")
        every { preferences.getDecryptedStringOrThrow("our_device_private_key") } throws failure
        every { preferences.getDecryptedString("our_device_private_key") } returns null
        justRun { preferences.putEncryptedString(any(), any()) }

        assertSame(failure, runCatching { provider.get() }.exceptionOrNull())
        verify(exactly = 0) { keyGenerator.generateRandomKeypair() }
    }

    @Test
    fun `memory visible failed writes must commit before the identity is cached`() = runTest {
        var stored: String? = null
        val failure = IllegalStateException("device key was not committed")
        every { preferences.getDecryptedStringOrThrow("our_device_private_key") } answers { stored }
        every { preferences.putEncryptedStringCommitted("our_device_private_key", any()) } answers {
            stored = secondArg()
            throw failure
        }

        repeat(2) {
            assertSame(failure, runCatching { provider.get() }.exceptionOrNull())
        }
        justRun { preferences.putEncryptedStringCommitted("our_device_private_key", any()) }
        val persisted = provider.get()
        every { preferences.getDecryptedStringOrThrow(any()) } throws IllegalStateException("cached key must be reused")

        assertEquals(persisted, provider.get())
        verify(exactly = 1) { keyGenerator.generateRandomKeypair() }
        verify(exactly = 3) { preferences.putEncryptedStringCommitted("our_device_private_key", any()) }
    }
}
