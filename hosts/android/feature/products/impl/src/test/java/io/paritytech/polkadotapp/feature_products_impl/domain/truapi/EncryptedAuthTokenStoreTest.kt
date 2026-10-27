package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.AuthToken
import uniffi.truapi.HostRejection

class EncryptedAuthTokenStoreTest {
    private val prefs = MapEncryptedPreferences()
    private val token = AuthToken(token = "jwt", refreshToken = "refresh", expiresAt = 1_900_000_000_000uL)

    private fun TestScope.store(preferences: EncryptedPreferences = prefs) =
        EncryptedAuthTokenStore(preferences, Gson(), testDispatchers())

    @Test
    fun `a kept token reads back under its key and counts only for its product`() = runTest {
        val store = store()

        store.storeAuthToken("wallet.dot", "alice", "k1", token)
        store.storeAuthToken("shop.dot", "alice", "k2", token)

        assertEquals(
            Triple(token, null, 1u),
            Triple(store.authToken("wallet.dot", "k1"), store.authToken("wallet.dot", "missing"), store.authTokenCount("wallet.dot", "alice")),
        )
    }

    // The core asks to clear one product when its state is cleared, and every product on sign-out.
    @Test
    fun `clearing drops one product's tokens or every token`() = runTest {
        val store = store()
        store.storeAuthToken("wallet.dot", "alice", "k1", token)
        store.storeAuthToken("shop.dot", "alice", "k2", token)

        store.clearAuthTokens("wallet.dot")
        val afterProduct = store.authToken("wallet.dot", "k1") to store.authToken("shop.dot", "k2")
        store.clearAuthTokens(null)

        assertEquals(null to token, afterProduct)
        assertEquals(0u, store.authTokenCount("shop.dot", "alice"))
        assertEquals(emptyMap<String, String>(), prefs.values)
    }

    @Test
    fun `removing a token takes it out of the count`() = runTest {
        val store = store()
        store.storeAuthToken("wallet.dot", "alice", "k1", token)

        store.removeAuthToken("wallet.dot", "k1")
        store.removeAuthToken("wallet.dot", "k1")

        assertNull(store.authToken("wallet.dot", "k1"))
        assertEquals(0u, store.authTokenCount("wallet.dot", "alice"))
    }

    // The core caps each account separately, so another account's tokens must not count.
    @Test
    fun `the count covers one account`() = runTest {
        val store = store()
        store.storeAuthToken("wallet.dot", "alice", "k1", token)
        store.storeAuthToken("wallet.dot", "bob", "k2", token)
        store.storeAuthToken("wallet.dot", "bob", "k3", token)

        assertEquals(1u to 2u, store.authTokenCount("wallet.dot", "alice") to store.authTokenCount("wallet.dot", "bob"))
    }

    // A token whose write failed must not stay in the index as a phantom, and a token that did land
    // must always be in the index, so clearing finds it.
    @Test
    fun `a failed token write leaves the index as it was`() = runTest {
        val preferences = MapEncryptedPreferences(dropWritesTo = { it.startsWith("truapi/auth-token/") })
        val store = store(preferences)

        val failure = runCatching { store.storeAuthToken("wallet.dot", "alice", "k1", token) }.exceptionOrNull()

        assertEquals(HostRejection.Rejected::class to 0u, failure?.let { it::class } to store.authTokenCount("wallet.dot", "alice"))
        assertEquals(emptyMap<String, String>(), preferences.values.filterValues { it.isNotEmpty() })
    }

    // A failed decrypt reads as an empty string. Taking that index as empty would orphan the tokens
    // it lists, so every call that needs the index fails and the kept entry stays listed.
    @Test
    fun `an index that does not decode fails instead of reading as empty`() = runTest {
        val store = store()
        store.storeAuthToken("wallet.dot", "alice", "k1", token)
        val index = prefs.values.getValue("truapi/auth-token-index")
        prefs.values["truapi/auth-token-index"] = ""

        val failures = listOf(
            runCatching { store.storeAuthToken("wallet.dot", "alice", "k2", token) },
            runCatching { store.authTokenCount("wallet.dot", "alice") },
            runCatching { store.clearAuthTokens(null) },
        ).map { it.exceptionOrNull()?.let { failure -> failure::class } }

        assertEquals(List(3) { HostRejection.Rejected::class }, failures)
        assertEquals(setOf("truapi/auth-token/k1", "truapi/auth-token-index"), prefs.values.keys)
        prefs.values["truapi/auth-token-index"] = index
        assertEquals(1u, store.authTokenCount("wallet.dot", "alice"))
    }

    // The encrypt path reports a failure by storing an empty string, which must not pass as kept.
    @Test
    fun `a write that did not land is reported`() = runTest {
        val store = store(MapEncryptedPreferences(dropWrites = true))

        val failure = runCatching { store.storeAuthToken("wallet.dot", "alice", "k1", token) }.exceptionOrNull()

        assertEquals(HostRejection.Rejected::class, failure?.let { it::class })
    }
}

private class MapEncryptedPreferences(
    private val dropWrites: Boolean = false,
    private val dropWritesTo: (String) -> Boolean = { dropWrites },
) : EncryptedPreferences {
    val values = mutableMapOf<String, String>()

    override fun putEncryptedString(field: String, value: String) {
        values[field] = if (dropWritesTo(field)) "" else value
    }

    override fun getDecryptedString(field: String): String? = values[field]

    override fun hasKey(field: String): Boolean = values.containsKey(field)

    override fun removeKey(field: String) {
        values.remove(field)
    }

    override fun decryptedStringFlow(field: String): Flow<String?> = flowOf(values[field])
}
