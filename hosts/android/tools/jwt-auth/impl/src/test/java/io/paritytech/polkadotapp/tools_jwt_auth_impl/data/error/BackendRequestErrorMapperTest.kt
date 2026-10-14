package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.error

import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.toBackendRequestError
import kotlinx.coroutines.CancellationException
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test
import retrofit2.HttpException
import retrofit2.Response
import java.io.IOException
import java.net.SocketTimeoutException
import java.net.UnknownHostException

class BackendRequestErrorMapperTest {
    @Test
    fun `401 is unauthorized rather than an integrity failure`() {
        // The old mapToIntegrityIfNeeded blanket-mapped every 401 to an integrity failure,
        // which misreported an ordinary expired JWT.
        assertEquals(
            BackendRequestError.Auth(AuthError.Unauthorized),
            httpException(401).toBackendRequestError()
        )
    }

    @Test
    fun `server codes carry the status`() {
        assertEquals(BackendRequestError.Server(500), httpException(500).toBackendRequestError())
        assertEquals(BackendRequestError.Server(503), httpException(503).toBackendRequestError())
        assertEquals(BackendRequestError.Server(404), httpException(404).toBackendRequestError())
    }

    @Test
    fun `transport failures are no connection`() {
        assertEquals(BackendRequestError.NoConnection, UnknownHostException().toBackendRequestError())
        assertEquals(BackendRequestError.NoConnection, SocketTimeoutException().toBackendRequestError())
        assertEquals(BackendRequestError.NoConnection, IOException("socket closed").toBackendRequestError())
    }

    @Test
    fun `auth error wrapped in an IOException is unwrapped`() {
        // BearerTokenInterceptor and BearerTokenAuthenticator can only throw IOException,
        // so the typed cause always arrives nested.
        val wrapped = IOException("JWT acquisition failed", AuthError.TokenRefreshFailed)

        assertEquals(
            BackendRequestError.Auth(AuthError.TokenRefreshFailed),
            wrapped.toBackendRequestError()
        )
    }

    @Test
    fun `integrity error wrapped in an IOException is lifted into an auth error`() {
        val wrapped = IOException(
            "Failed to attach integrity headers",
            IntegrityError.AttestationUnavailable
        )

        assertEquals(
            BackendRequestError.Auth(AuthError.Integrity(IntegrityError.AttestationUnavailable)),
            wrapped.toBackendRequestError()
        )
    }

    @Test
    fun `nested cause wins over the outer transport shape`() {
        // Without cause-walking this would classify as NoConnection and lose the reason.
        val doublyWrapped = IOException(
            "outer",
            IOException("inner", AuthError.Integrity(IntegrityError.AttestationRejected))
        )

        assertEquals(
            BackendRequestError.Auth(AuthError.Integrity(IntegrityError.AttestationRejected)),
            doublyWrapped.toBackendRequestError()
        )
    }

    @Test
    fun `already typed errors pass through untouched`() {
        val error = BackendRequestError.Malformed

        assertSame(error, error.toBackendRequestError())
    }

    @Test
    fun `cancellation is never reclassified`() {
        val cancellation = CancellationException("scope died")

        assertSame(cancellation, cancellation.toBackendRequestError())
    }

    @Test
    fun `unrecognised throwable is unknown`() {
        assertEquals(BackendRequestError.Unknown, IllegalStateException("boom").toBackendRequestError())
    }

    private fun httpException(code: Int): HttpException {
        val body = "{}".toResponseBody("application/json".toMediaType())
        return HttpException(Response.error<Unit>(code, body))
    }
}
