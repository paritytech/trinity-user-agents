package io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error

import com.google.gson.JsonParseException
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import kotlinx.coroutines.CancellationException
import retrofit2.HttpException
import timber.log.Timber
import java.io.IOException

private const val HTTP_UNAUTHORIZED = 401

/**
 * Transport-level failure taxonomy shared by every backend call. Features narrow this into
 * their own domain error rather than surfacing it directly.
 */
sealed class BackendRequestError(message: String) : Throwable(message) {
    data class Auth(val error: AuthError) : BackendRequestError("auth failed")

    data object NoConnection : BackendRequestError("no connection")

    data class Server(val code: Int) : BackendRequestError("server error")

    data object Malformed : BackendRequestError("malformed response")

    data object Unknown : BackendRequestError("unknown backend failure")

    override fun fillInStackTrace(): Throwable = this
}

/**
 * Classifies a raw network throwable. Interceptors may only throw [IOException], so a typed
 * cause always arrives nested — the cause chain is walked before falling back to transport shape.
 */
fun Throwable.toBackendRequestError(): Throwable {
    if (this is CancellationException) return this
    if (this is BackendRequestError) return this

    findCause<AuthError>()?.let { return BackendRequestError.Auth(it) }
    findCause<IntegrityError>()?.let { return BackendRequestError.Auth(AuthError.Integrity(it)) }

    return when {
        this is HttpException -> when (code()) {
            HTTP_UNAUTHORIZED -> BackendRequestError.Auth(AuthError.Unauthorized)
            else -> BackendRequestError.Server(code())
        }

        this is JsonParseException -> BackendRequestError.Malformed

        this is IOException -> BackendRequestError.NoConnection

        else -> {
            // Payload-free variants cannot carry the cause, and this is the last place with it.
            Timber.e(this, "Unclassified backend failure")
            BackendRequestError.Unknown
        }
    }
}

inline fun <reified T : Throwable> Throwable.findCause(): T? {
    var current: Throwable? = this
    while (current != null) {
        if (current is T) return current
        if (current === current.cause) return null
        current = current.cause
    }
    return null
}
