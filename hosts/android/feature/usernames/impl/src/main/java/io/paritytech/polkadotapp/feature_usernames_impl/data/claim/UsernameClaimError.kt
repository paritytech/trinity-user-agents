package io.paritytech.polkadotapp.feature_usernames_impl.data.claim

import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.toBackendRequestError
import retrofit2.HttpException

private const val HTTP_CONFLICT = 409

class UsernameAlreadyClaimedException : RuntimeException()

class QueueRateLimitedException(val retryAfterSeconds: Long?) : RuntimeException()

fun mapClaimUsernameError(throwable: Throwable): Throwable = when {
    // 409 is control flow, not an error surface — the interactor recovers a fresh suffix from it.
    throwable is HttpException && throwable.response()?.code() == HTTP_CONFLICT -> UsernameAlreadyClaimedException()

    else -> throwable.toBackendRequestError()
}
