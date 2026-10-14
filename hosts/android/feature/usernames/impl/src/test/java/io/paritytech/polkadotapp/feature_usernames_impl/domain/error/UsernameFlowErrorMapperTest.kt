package io.paritytech.polkadotapp.feature_usernames_impl.domain.error

import io.paritytech.polkadotapp.feature_backup_api.domain.error.ImportFromBackupError
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import kotlinx.coroutines.CancellationException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class UsernameFlowErrorMapperTest {
    @Test
    fun `attestation variants keep their distinct advice`() {
        // Each maps to a different message and, for the first two, a different destination.
        assertEquals(
            UsernameFlowError.VerificationUnavailable,
            integrityFailure(IntegrityError.AttestationUnavailable).toUsernameFlowError()
        )
        assertEquals(
            UsernameFlowError.VerificationRejected,
            integrityFailure(IntegrityError.AttestationRejected).toUsernameFlowError()
        )
        assertEquals(
            UsernameFlowError.VerificationBusy,
            integrityFailure(IntegrityError.AttestationTransient).toUsernameFlowError()
        )
        assertEquals(
            UsernameFlowError.Unknown,
            integrityFailure(IntegrityError.Unknown).toUsernameFlowError()
        )
    }

    @Test
    fun `plain auth failures are server side`() {
        assertEquals(
            UsernameFlowError.Server,
            BackendRequestError.Auth(AuthError.Unauthorized).toUsernameFlowError()
        )
        assertEquals(
            UsernameFlowError.Server,
            BackendRequestError.Auth(AuthError.TokenRefreshFailed).toUsernameFlowError()
        )
    }

    @Test
    fun `transport and protocol failures flatten`() {
        assertEquals(UsernameFlowError.NoConnection, BackendRequestError.NoConnection.toUsernameFlowError())
        assertEquals(UsernameFlowError.Server, BackendRequestError.Server(500).toUsernameFlowError())
        assertEquals(UsernameFlowError.Server, BackendRequestError.Malformed.toUsernameFlowError())
        assertEquals(UsernameFlowError.Unknown, BackendRequestError.Unknown.toUsernameFlowError())
    }

    @Test
    fun `already flattened errors pass through`() {
        assertSame(
            UsernameFlowError.NoConnection,
            UsernameFlowError.NoConnection.toUsernameFlowError()
        )
    }

    @Test
    fun `coroutine cancellation is never reclassified`() {
        val cancellation = CancellationException("scope died")

        assertSame(cancellation, cancellation.toUsernameFlowError())
    }

    @Test
    fun `a foreign cancellation becomes the flow-level variant`() {
        // The interactor must only ever fail with UsernameFlowError - callers should not have to
        // recall that a backup type can surface here.
        assertEquals(
            UsernameFlowError.Cancelled,
            ImportFromBackupError.Cancelled.toUsernameFlowError()
        )
    }

    @Test
    fun `unrecognised throwable is unknown`() {
        assertEquals(UsernameFlowError.Unknown, IllegalStateException("boom").toUsernameFlowError())
    }

    @Test
    fun `asUsernameFlowError is total`() {
        assertEquals(UsernameFlowError.Unknown, CancellationException("x").asUsernameFlowError())
        assertEquals(UsernameFlowError.Cancelled, ImportFromBackupError.Cancelled.asUsernameFlowError())
        assertEquals(
            UsernameFlowError.NoConnection,
            BackendRequestError.NoConnection.asUsernameFlowError()
        )
    }

    private fun integrityFailure(error: IntegrityError): BackendRequestError =
        BackendRequestError.Auth(AuthError.Integrity(error))
}
