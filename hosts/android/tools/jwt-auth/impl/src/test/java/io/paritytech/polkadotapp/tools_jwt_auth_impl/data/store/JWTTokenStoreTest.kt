package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store

import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class JWTTokenStoreTest {
    private lateinit var store: JWTTokenStore
    private lateinit var prefs: FakeEncryptedPreferences

    @Before
    fun setUp() {
        prefs = FakeEncryptedPreferences()
        store = JWTTokenStore(prefs)
    }

    // MARK: - Access token

    @Test
    fun `fetchToken returns null when empty`() {
        assertNull(store.fetchToken())
    }

    @Test
    fun `saves and fetches token`() {
        store.saveToken("access-token")
        assertEquals("access-token", store.fetchToken())
    }

    @Test
    fun `overwrites existing token`() {
        store.saveToken("first")
        store.saveToken("second")
        assertEquals("second", store.fetchToken())
    }

    @Test
    fun `deleteToken clears stored token`() {
        store.saveToken("to-delete")
        store.deleteToken()
        assertNull(store.fetchToken())
    }

    // MARK: - Refresh token

    @Test
    fun `fetchRefreshToken returns null when empty`() {
        assertNull(store.fetchRefreshToken())
    }

    @Test
    fun `saves and fetches refresh token`() {
        store.saveRefreshToken("refresh-token")
        assertEquals("refresh-token", store.fetchRefreshToken())
    }

    @Test
    fun `overwrites existing refresh token`() {
        store.saveRefreshToken("first")
        store.saveRefreshToken("second")
        assertEquals("second", store.fetchRefreshToken())
    }

    @Test
    fun `deleteRefreshToken clears stored refresh token`() {
        store.saveRefreshToken("to-delete")
        store.deleteRefreshToken()
        assertNull(store.fetchRefreshToken())
    }

    // MARK: - Independence

    @Test
    fun `access and refresh tokens are independent`() {
        store.saveToken("access")
        store.saveRefreshToken("refresh")

        store.deleteToken()

        assertNull(store.fetchToken())
        assertEquals("refresh", store.fetchRefreshToken())
    }

    @Test
    fun `deleting refresh does not affect access`() {
        store.saveToken("access")
        store.saveRefreshToken("refresh")

        store.deleteRefreshToken()

        assertEquals("access", store.fetchToken())
        assertNull(store.fetchRefreshToken())
    }

    // MARK: - Delete all

    @Test
    fun `deleteAll clears both tokens`() {
        store.saveToken("access")
        store.saveRefreshToken("refresh")

        store.deleteAll()

        assertNull(store.fetchToken())
        assertNull(store.fetchRefreshToken())
    }

    @Test
    fun `deleteAll on empty store does not throw`() {
        store.deleteAll()
        assertNull(store.fetchToken())
        assertNull(store.fetchRefreshToken())
    }
}

private class FakeEncryptedPreferences : EncryptedPreferences {
    private val storage = mutableMapOf<String, String>()

    override fun putEncryptedString(field: String, value: String) {
        storage[field] = value
    }

    override fun getDecryptedString(field: String): String? = storage[field]

    override fun hasKey(field: String): Boolean = storage.containsKey(field)

    override fun removeKey(field: String) {
        storage.remove(field)
    }

    override fun decryptedStringFlow(field: String): Flow<String?> = flowOf(storage[field])
}
