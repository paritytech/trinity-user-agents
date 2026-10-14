package io.paritytech.polkadotapp.tools_integrity_impl.data

import com.google.android.play.core.integrity.IntegrityServiceException
import com.google.android.play.core.integrity.model.IntegrityErrorCode
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import kotlinx.coroutines.CancellationException

internal fun Throwable.toIntegrityError(): IntegrityError = when (this) {
    is CancellationException -> throw this
    is IntegrityError -> this
    is IntegrityServiceException -> playIntegrityErrorCodeToError(errorCode)
    else -> IntegrityError.Unknown
}

/**
 * Split from [toIntegrityError] because [IntegrityServiceException] has no public constructor —
 * the branching lives here so it can be exercised with plain error codes.
 */
internal fun playIntegrityErrorCodeToError(errorCode: Int): IntegrityError = when (errorCode) {
    IntegrityErrorCode.PLAY_STORE_NOT_FOUND,
    IntegrityErrorCode.PLAY_SERVICES_NOT_FOUND,
    IntegrityErrorCode.PLAY_STORE_VERSION_OUTDATED,
    IntegrityErrorCode.PLAY_SERVICES_VERSION_OUTDATED,
    IntegrityErrorCode.PLAY_STORE_ACCOUNT_NOT_FOUND,
    IntegrityErrorCode.APP_NOT_INSTALLED,
    IntegrityErrorCode.API_NOT_AVAILABLE,
    IntegrityErrorCode.CANNOT_BIND_TO_SERVICE -> IntegrityError.AttestationUnavailable

    IntegrityErrorCode.NETWORK_ERROR,
    IntegrityErrorCode.TOO_MANY_REQUESTS,
    IntegrityErrorCode.GOOGLE_SERVER_UNAVAILABLE,
    IntegrityErrorCode.CLIENT_TRANSIENT_ERROR,
    IntegrityErrorCode.INTERNAL_ERROR -> IntegrityError.AttestationTransient

    // Nonce and cloud-project failures are our misconfiguration, not a device limitation —
    // advising the user to switch build would be wrong.
    else -> IntegrityError.Unknown
}
