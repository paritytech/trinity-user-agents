package io.paritytech.polkadotapp.feature_usernames_impl.domain.error

import io.paritytech.polkadotapp.feature_backup_api.domain.error.ImportFromBackupError
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import kotlinx.coroutines.CancellationException
import timber.log.Timber

/**
 * Claim-flow failures, flattened so the screen resolves one to a string with a single-level
 * `when`. Variants are payload-free because this reaches Compose state.
 */
sealed class UsernameFlowError(message: String) : Throwable(message) {
    /** The user backed out — for example by dismissing the Google sign-in sheet. Shows nothing. */
    data object Cancelled : UsernameFlowError("cancelled by user")

    data object NoConnection : UsernameFlowError("no connection")

    data object VerificationUnavailable : UsernameFlowError("device cannot verify")

    data object VerificationRejected : UsernameFlowError("device verification refused")

    data object VerificationBusy : UsernameFlowError("verification temporarily unavailable")

    data object Server : UsernameFlowError("backend failed")

    data object Unknown : UsernameFlowError("unknown failure")

    override fun fillInStackTrace(): Throwable = this
}

/**
 * Boundary mapper for `mapError`. Coroutine cancellation passes through untouched; every other
 * failure becomes a [UsernameFlowError], so the interactor never hands a caller a foreign type.
 */
fun Throwable.toUsernameFlowError(): Throwable = when (this) {
    is CancellationException -> this
    is UsernameFlowError -> this
    ImportFromBackupError.Cancelled -> UsernameFlowError.Cancelled

    is BackendRequestError -> when (this) {
        is BackendRequestError.Auth -> error.toUsernameFlowError()
        BackendRequestError.NoConnection -> UsernameFlowError.NoConnection
        is BackendRequestError.Server -> UsernameFlowError.Server
        BackendRequestError.Malformed -> UsernameFlowError.Server
        BackendRequestError.Unknown -> UsernameFlowError.Unknown
    }

    else -> {
        // Payload-free variants cannot carry the cause, and this is the last place with it.
        Timber.e(this, "Unclassified username flow failure")
        UsernameFlowError.Unknown
    }
}

/** Total variant of [toUsernameFlowError], for the terminal sites that must render something. */
fun Throwable.asUsernameFlowError(): UsernameFlowError =
    toUsernameFlowError() as? UsernameFlowError ?: UsernameFlowError.Unknown

private fun AuthError.toUsernameFlowError(): UsernameFlowError = when (this) {
    is AuthError.Integrity -> when (error) {
        IntegrityError.AttestationUnavailable -> UsernameFlowError.VerificationUnavailable
        IntegrityError.AttestationRejected -> UsernameFlowError.VerificationRejected
        IntegrityError.AttestationTransient -> UsernameFlowError.VerificationBusy
        IntegrityError.Unknown -> UsernameFlowError.Unknown
    }

    // A rejected or unrefreshable token is not something the user can act on.
    AuthError.TokenRefreshFailed -> UsernameFlowError.Server
    AuthError.Unauthorized -> UsernameFlowError.Server
}
