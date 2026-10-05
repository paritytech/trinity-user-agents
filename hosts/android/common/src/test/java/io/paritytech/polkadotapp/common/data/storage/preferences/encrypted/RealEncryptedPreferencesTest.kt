package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Editor
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test

class RealEncryptedPreferencesTest {
    private val editor = mockk<Editor>(relaxed = true)
    private val preferences = mockk<Preferences> { every { edit() } returns editor }
    private val cipher = mockk<EncryptionUtil>()
    private val store = RealEncryptedPreferences(preferences, cipher)

    @Test
    fun `a failed disk commit fails secret writes and removals`() {
        every { cipher.encryptOrThrow("secret") } returns "ciphertext"
        every { editor.commit() } returns false
        assertThrows(IllegalStateException::class.java) { store.putEncryptedStringCommitted("key", "secret") }
        assertThrows(IllegalStateException::class.java) { store.removeKeyCommitted("key") }
        verify(exactly = 2) { editor.commit() }
        verify(exactly = 0) { editor.apply() }
    }

    @Test
    fun `protected storage corruption cannot become absence`() {
        val corrupt = IllegalStateException("authentication failed")
        every { preferences.getString("key") } returns "corrupt"
        every { cipher.decryptOrThrow("corrupt") } throws corrupt
        val failure = runCatching { store.getDecryptedStringOrThrow("key") }.exceptionOrNull()
        assertSame(corrupt, failure)
    }
}
