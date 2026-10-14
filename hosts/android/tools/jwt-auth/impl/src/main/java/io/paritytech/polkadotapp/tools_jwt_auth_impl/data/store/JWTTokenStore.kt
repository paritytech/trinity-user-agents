package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store

import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class JWTTokenStore @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences
) {
    companion object {
        private const val KEY_ACCESS_TOKEN = "jwt_access_token"
        private const val KEY_REFRESH_TOKEN = "jwt_refresh_token"
    }

    fun saveToken(token: String) {
        encryptedPreferences.putEncryptedString(KEY_ACCESS_TOKEN, token)
    }

    fun fetchToken(): String? {
        return encryptedPreferences.getDecryptedString(KEY_ACCESS_TOKEN)?.takeUnless { it.isEmpty() }
    }

    fun deleteToken() {
        encryptedPreferences.removeKey(KEY_ACCESS_TOKEN)
    }

    fun saveRefreshToken(token: String) {
        encryptedPreferences.putEncryptedString(KEY_REFRESH_TOKEN, token)
    }

    fun fetchRefreshToken(): String? {
        return encryptedPreferences.getDecryptedString(KEY_REFRESH_TOKEN)?.takeUnless { it.isEmpty() }
    }

    fun deleteRefreshToken() {
        encryptedPreferences.removeKey(KEY_REFRESH_TOKEN)
    }

    fun deleteAll() {
        deleteToken()
        deleteRefreshToken()
    }
}
