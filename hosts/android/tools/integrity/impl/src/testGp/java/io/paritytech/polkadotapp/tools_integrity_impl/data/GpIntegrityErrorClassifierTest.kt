package io.paritytech.polkadotapp.tools_integrity_impl.data

import com.google.android.play.core.integrity.model.IntegrityErrorCode
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import org.junit.Assert.assertEquals
import org.junit.Test

class GpIntegrityErrorClassifierTest {

    @Test
    fun `missing play store or services cannot attest`() {
        val unavailableCodes = listOf(
            IntegrityErrorCode.PLAY_STORE_NOT_FOUND,
            IntegrityErrorCode.PLAY_SERVICES_NOT_FOUND,
            IntegrityErrorCode.PLAY_STORE_VERSION_OUTDATED,
            IntegrityErrorCode.PLAY_SERVICES_VERSION_OUTDATED,
            IntegrityErrorCode.PLAY_STORE_ACCOUNT_NOT_FOUND,
            IntegrityErrorCode.APP_NOT_INSTALLED,
            IntegrityErrorCode.API_NOT_AVAILABLE,
            IntegrityErrorCode.CANNOT_BIND_TO_SERVICE,
        )

        unavailableCodes.forEach { code ->
            assertEquals(
                "error code $code should be AttestationUnavailable",
                IntegrityError.AttestationUnavailable,
                playIntegrityErrorCodeToError(code)
            )
        }
    }

    @Test
    fun `network and rate limit failures are transient`() {
        val transientCodes = listOf(
            IntegrityErrorCode.NETWORK_ERROR,
            IntegrityErrorCode.TOO_MANY_REQUESTS,
            IntegrityErrorCode.GOOGLE_SERVER_UNAVAILABLE,
            IntegrityErrorCode.CLIENT_TRANSIENT_ERROR,
            IntegrityErrorCode.INTERNAL_ERROR,
        )

        transientCodes.forEach { code ->
            assertEquals(
                "error code $code should be AttestationTransient",
                IntegrityError.AttestationTransient,
                playIntegrityErrorCodeToError(code)
            )
        }
    }

    @Test
    fun `misconfiguration is not reported as a device problem`() {
        // A bad nonce or cloud project number is our bug, not the user's device — telling them to
        // install a vanilla build would be wrong.
        val configurationCodes = listOf(
            IntegrityErrorCode.CLOUD_PROJECT_NUMBER_IS_INVALID,
            IntegrityErrorCode.NONCE_TOO_SHORT,
            IntegrityErrorCode.NONCE_TOO_LONG,
            IntegrityErrorCode.NONCE_IS_NOT_BASE64,
            IntegrityErrorCode.APP_UID_MISMATCH,
        )

        configurationCodes.forEach { code ->
            assertEquals(
                "error code $code should be Unknown",
                IntegrityError.Unknown,
                playIntegrityErrorCodeToError(code)
            )
        }
    }

    @Test
    fun `already typed errors pass through`() {
        assertEquals(
            IntegrityError.AttestationRejected,
            IntegrityError.AttestationRejected.toIntegrityError()
        )
    }

    @Test
    fun `unrecognised throwable is unknown`() {
        assertEquals(IntegrityError.Unknown, IllegalStateException("boom").toIntegrityError())
    }
}
