package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import io.parity.truapi.AuthTokenHostBridge
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.truapi.AuthToken
import uniffi.truapi.HostRejection
import javax.inject.Inject
import javax.inject.Singleton

/**
 * [AuthTokenHostBridge] over the app's Keystore-encrypted preferences: one entry per token, under
 * the opaque key the core derives from the signed-in account, the product and its service, so no
 * lookup here reaches another account's or product's token.
 *
 * Preferences cannot be listed, so an encrypted index maps each key to its product and account
 * for counting and clearing. A key enters the index before its token is written and leaves it if
 * the write fails, so clearing always finds every token. The core clears tokens whenever a session
 * ends and when a product's state is cleared.
 */
@Singleton
class EncryptedAuthTokenStore @Inject constructor(
    private val preferences: EncryptedPreferences,
    private val gson: Gson,
    private val dispatchers: CoroutineDispatchers,
) : AuthTokenHostBridge {
    private val indexLock = Mutex()

    override suspend fun storeAuthToken(productId: String, account: String, key: String, token: AuthToken) = io {
        indexLock.withLock {
            val index = readIndex()
            writeIndex(index + (key to IndexRow(productId, account)))
            runCatching { write(entryField(key), gson.toJson(StoredAuthToken(token))) }.onFailure {
                if (key !in index) writeIndex(index)
                throw it
            }
            Unit
        }
    }

    /** An entry that no longer decodes reads as absent, so the product signs in again. */
    override suspend fun authToken(productId: String, key: String): AuthToken? = io {
        preferences.getDecryptedString(entryField(key))
            ?.let { runCatching { gson.fromJson(it, StoredAuthToken::class.java).toAuthToken() }.getOrNull() }
    }

    override suspend fun removeAuthToken(productId: String, key: String) = io {
        indexLock.withLock {
            preferences.removeKey(entryField(key))
            writeIndex(readIndex() - key)
        }
    }

    override suspend fun authTokenCount(productId: String, account: String): UInt = io {
        indexLock.withLock { readIndex().values.count { it == IndexRow(productId, account) }.toUInt() }
    }

    override suspend fun clearAuthTokens(productId: String?) = io {
        indexLock.withLock {
            val index = readIndex()
            val dropped = index.filterValues { productId == null || it.productId == productId }.keys
            dropped.forEach { preferences.removeKey(entryField(it)) }
            writeIndex(index - dropped)
        }
    }

    // An empty index is removed, so one that is there but does not decode is lost, not empty: reading
    // it as empty would let the next write drop every token it lists from clearing and the count.
    private fun readIndex(): Map<String, IndexRow> {
        val stored = preferences.getDecryptedString(INDEX_FIELD) ?: return emptyMap()
        return runCatching { gson.fromJson<Map<String, IndexRow>>(stored, INDEX_TYPE) }.getOrNull()
            ?: throw HostRejection.Rejected("the auth token index is unreadable")
    }

    private fun writeIndex(index: Map<String, IndexRow>) {
        if (index.isEmpty()) preferences.removeKey(INDEX_FIELD) else write(INDEX_FIELD, gson.toJson(index))
    }

    // The encrypt path swallows its own failures, so reading the value back is the only proof it landed.
    private fun write(field: String, value: String) {
        preferences.putEncryptedString(field, value)
        if (preferences.getDecryptedString(field) != value) {
            throw HostRejection.Rejected("failed to persist an auth token")
        }
    }

    private suspend fun <T> io(block: suspend () -> T): T = withContext(dispatchers.io) { block() }

    private fun entryField(key: String) = "$ENTRY_PREFIX$key"

    private companion object {
        const val ENTRY_PREFIX = "truapi/auth-token/"
        const val INDEX_FIELD = "truapi/auth-token-index"
        val INDEX_TYPE = object : TypeToken<Map<String, IndexRow>>() {}.type
    }
}

/** Whose a kept token is: the product and the core's opaque account tag. */
private data class IndexRow(val productId: String, val account: String)

/** The stored form of an [AuthToken]. Its [toString] never shows either secret. */
private class StoredAuthToken(
    val token: String,
    val refreshToken: String?,
    val expiresAt: Long?,
) {
    constructor(authToken: AuthToken) : this(
        authToken.token,
        authToken.refreshToken,
        authToken.expiresAt?.toLong(),
    )

    fun toAuthToken() = AuthToken(token = token, refreshToken = refreshToken, expiresAt = expiresAt?.toULong())

    override fun toString() = "StoredAuthToken(token=<redacted>, refreshToken=<redacted>, expiresAt=$expiresAt)"
}
