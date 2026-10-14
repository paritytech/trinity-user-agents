package io.parity.truapi

import android.content.SharedPreferences
import uniffi.truapi.HostRejection
import uniffi.truapi.HostLocalStorageReadException

/**
 * Test double for the diagnostics harness. Production hosts implement
 * [HostStorage] themselves, encrypted at rest.
 *
 * Product-scoped storage over [SharedPreferences], values stored hex-encoded. Uses `commit()` so a failed write surfaces as the declared
 * [HostLocalStorageReadException] instead of being swallowed by async `apply()`. A
 * corrupt entry reads back as a miss.
 */
class PrefsHostStorage(private val prefs: SharedPreferences) : HostStorage {
    override suspend fun read(key: String): ByteArray? = decodeOrNull(prefs.getString(key, null))
    override suspend fun write(key: String, value: ByteArray) {
        if (!prefs.edit().putString(key, bytesToHex(value)).commit()) {
            throw storageFailure("failed to persist product storage key")
        }
    }
    override suspend fun clear(key: String) {
        if (!prefs.edit().remove(key).commit()) {
            throw storageFailure("failed to clear product storage key")
        }
    }
}

/**
 * Holds the core's auth and pairing session state plus persisted permission
 * decisions. Uses `commit()` because a write the core believes succeeded must
 * not be lost on process death; failures surface as the declared [HostRejection].
 */
class PrefsHostCoreStorage(private val prefs: SharedPreferences) : HostCoreStorage {
    override suspend fun read(key: ByteArray): ByteArray? =
        decodeOrNull(prefs.getString(bytesToHex(key), null))
    override suspend fun write(key: ByteArray, value: ByteArray) {
        if (!prefs.edit().putString(bytesToHex(key), bytesToHex(value)).commit()) {
            throw HostRejection.Rejected("failed to persist core storage key")
        }
    }
    override suspend fun clear(key: ByteArray) {
        if (!prefs.edit().remove(bytesToHex(key)).commit()) {
            throw HostRejection.Rejected("failed to clear core storage key")
        }
    }
}

private fun storageFailure(reason: String): HostLocalStorageReadException =
    HostLocalStorageReadException.Unknown(reason)

@OptIn(ExperimentalStdlibApi::class)
private fun bytesToHex(bytes: ByteArray): String = bytes.toHexString()

@OptIn(ExperimentalStdlibApi::class)
private fun decodeOrNull(hex: String?): ByteArray? =
    hex?.let { runCatching { it.hexToByteArray() }.getOrNull() }
