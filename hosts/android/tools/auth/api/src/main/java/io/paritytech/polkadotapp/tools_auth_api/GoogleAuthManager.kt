package io.paritytech.polkadotapp.tools_auth_api

interface GoogleAuthManager {
    suspend fun isAuthorized(): Boolean

    suspend fun signOut()

    suspend fun <T> runAuthenticated(
        scope: String?,
        action: suspend (String?) -> Result<T>
    ): Result<T>
}
