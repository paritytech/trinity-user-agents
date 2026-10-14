package io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error

import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError

/**
 * Failures of the bearer-token acquisition path. [Integrity] wraps a payload-free
 * [IntegrityError] singleton, so equality stays structural all the way to Compose state.
 */
sealed class AuthError(message: String) : Throwable(message) {
    data class Integrity(val error: IntegrityError) : AuthError("integrity check failed")

    data object TokenRefreshFailed : AuthError("token refresh failed")

    data object Unauthorized : AuthError("unauthorized")

    override fun fillInStackTrace(): Throwable = this
}
