package io.paritytech.polkadotapp.tools_auth_impl.google

import io.paritytech.polkadotapp.tools_auth_api.GoogleAuthManager
import javax.inject.Inject

class NoOpGoogleAuthManager @Inject constructor() : GoogleAuthManager {
    override suspend fun isAuthorized(): Boolean = false

    override suspend fun signOut() {}

    override suspend fun <T> runAuthenticated(
        scope: String?,
        action: suspend (String?) -> Result<T>
    ): Result<T> {
        return Result.failure(UnsupportedOperationException("Google auth is not available in vanilla build"))
    }
}
