package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.HostCoreStorage
import io.parity.truapi.HostStorage
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import uniffi.truapi.HostRejection
import uniffi.truapi.HostLocalStorageReadException
import java.text.Normalizer
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Product-scoped storage for the Rust core, encrypted at rest.
 *
 * Reads go to the product the core named as the key's owner, which is another
 * product on a granted foreign read. Writes and clears stay in [productId]'s
 * namespace.
 */
class EncryptedHostStorage(
    private val preferences: EncryptedPreferences,
    private val productId: String,
) : HostStorage {
    private val normalizedProductId = normalizeProductId(productId)

    override suspend fun read(key: String): ByteArray? = readValue(preferences, "${productStorageNamespace(ownerOf(key))}/$key")

    override suspend fun write(key: String, value: ByteArray) {
        writeValue(preferences, qualify(key), value)
            ?.let { throw storageFailure("product storage: $it") }
    }

    override suspend fun clear(key: String) {
        runCatching { preferences.removeKey(qualify(key)) }
            .getOrElse { throw storageFailure("failed to clear product storage key: ${it.message}") }
    }

    private fun qualify(key: String) = "${productStorageNamespace(productId)}/$key"

    private fun ownerOf(key: String): String =
        productStorageKeyOwner(key)?.takeUnless { it == normalizedProductId } ?: productId
}

/**
 * Core-owned storage: auth session, pairing identity, and persisted permission
 * decisions.
 *
 * Deliberately *not* product-scoped. The pairing identity belongs to the user,
 * not to a product, and scoping it per product would make every product demand
 * its own pairing. The core disambiguates internally through the SCALE-encoded
 * `CoreStorageKey` it passes here.
 */
class EncryptedHostCoreStorage(
    private val preferences: EncryptedPreferences,
) : HostCoreStorage {
    override val storageIdentifier: String = "${preferences.storageIdentifier}/$CORE_NAMESPACE"

    override suspend fun read(key: ByteArray): ByteArray? = coreCall {
        val field = qualify(key)
        if (!preferences.hasKey(field)) return@coreCall null
        val stored = requireNotNull(preferences.getDecryptedString(field))
        check(stored.startsWith(VALUE_TAG)) { "Invalid core storage envelope" }
        requireNotNull(decodeOrNull(stored.removePrefix(VALUE_TAG))) { "Invalid core storage bytes" }
    }

    override suspend fun write(key: ByteArray, value: ByteArray) = coreCall {
        preferences.putEncryptedStringCommitted(qualify(key), VALUE_TAG + value.toHex())
        changes.update { it + 1 }
    }

    override suspend fun clear(key: ByteArray) = coreCall {
        preferences.removeKeyCommitted(qualify(key))
        changes.update { it + 1 }
    }

    private inline fun <T> coreCall(block: () -> T): T = try {
        block()
    } catch (error: Exception) {
        throw HostRejection.Rejected("Private host storage unavailable")
    }

    override suspend fun keys(): List<ByteArray> = preferences.keys()
        .filter { it.startsWith("$CORE_NAMESPACE/") }
        .map { key ->
            decodeOrNull(key.removePrefix("$CORE_NAMESPACE/"))
                ?: throw HostRejection.Rejected("invalid persisted core storage key")
        }

    private fun qualify(key: ByteArray) = "$CORE_NAMESPACE/${key.toHex()}"

    companion object {
        private const val CORE_NAMESPACE = "truapi/core"
        private val changes = MutableStateFlow(0L)
        val storageChanges = changes.asStateFlow()
    }
}

/** Namespace for one product's core-facing local storage. */
fun productStorageNamespace(productId: String): String = "truapi/product/$productId"

private const val PRODUCT_STORAGE_KEY_PREFIX = "truapi:product-storage:v1:"

/** Mirrors `ProductStorageKey::decode` in truapi: `truapi:product-storage:v1:<byte length>:<product id>:<key>`. */
internal fun productStorageKeyOwner(key: String): String? {
    if (!key.startsWith(PRODUCT_STORAGE_KEY_PREFIX)) return null
    val rest = key.removePrefix(PRODUCT_STORAGE_KEY_PREFIX).encodeToByteArray()
    val colon = rest.indexOf(':'.code.toByte()).takeIf { it > 0 } ?: return null
    val length = rest.decodeToString(0, colon).toIntOrNull()?.takeIf { it > 0 } ?: return null
    val start = colon + 1
    if (length > rest.size - start - 1 || rest[start + length] != ':'.code.toByte()) return null
    return runCatching { rest.decodeToString(start, start + length, throwOnInvalidSequence = true) }.getOrNull()
}

/** Matches the core's `normalize_product_identifier`, the form written into keys. */
internal fun normalizeProductId(productId: String): String =
    Normalizer.normalize(productId.trim(), Normalizer.Form.NFC).lowercase()

/**
 * `EncryptionUtil` reports both a failed encrypt and a failed decrypt by
 * returning an empty string, and it also refuses to encrypt an empty input, so
 * a write that silently failed is otherwise indistinguishable from a value that
 * is legitimately empty. Tagging the plaintext makes the two tellable apart: an
 * untagged read is a failure and must surface as a miss rather than as empty
 * bytes the core would treat as real.
 */
private const val VALUE_TAG = "v1:"

private fun readValue(preferences: EncryptedPreferences, key: String): ByteArray? {
    val stored = preferences.getDecryptedString(key) ?: return null
    if (!stored.startsWith(VALUE_TAG)) return null
    return decodeOrNull(stored.removePrefix(VALUE_TAG))
}

/** Returns null on success, or a reason to report to the core. */
private fun writeValue(preferences: EncryptedPreferences, key: String, value: ByteArray): String? {
    val tagged = VALUE_TAG + value.toHex()
    val failure = runCatching { preferences.putEncryptedString(key, tagged) }.exceptionOrNull()
    if (failure != null) return "failed to persist $key: ${failure.message}"

    // The encrypt path swallows its own exceptions, so the only reliable
    // signal that the value landed is reading it back.
    return if (preferences.getDecryptedString(key) == tagged) null else "failed to persist $key"
}

private fun storageFailure(reason: String): HostLocalStorageReadException =
    HostLocalStorageReadException.Unknown(reason)

@OptIn(ExperimentalStdlibApi::class)
private fun ByteArray.toHex(): String = toHexString()

@OptIn(ExperimentalStdlibApi::class)
private fun decodeOrNull(hex: String?): ByteArray? =
    hex?.let { runCatching { it.hexToByteArray() }.getOrNull() }
