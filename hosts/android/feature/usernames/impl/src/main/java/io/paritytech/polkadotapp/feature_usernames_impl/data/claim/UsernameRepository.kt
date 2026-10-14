package io.paritytech.polkadotapp.feature_usernames_impl.data.claim

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.flatten
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.FoundUser
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.UsernameApi
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.RegistrationOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameAvailableRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimDotNsRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimResponse
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.toDomain
import io.paritytech.polkadotapp.feature_usernames_impl.domain.UsernamesChainProvider
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameParams
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.RegistrationQueueStatus
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameClaimResult
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.toBackendRequestError
import retrofit2.HttpException
import javax.inject.Inject

interface UsernameRepository {
    suspend fun checkUsernameAvailable(username: Username): Result<UsernameAvailabilityState>

    suspend fun claimUsername(params: ClaimUsernameParams): Result<UsernameClaimResult>

    suspend fun getRegistrationQueueStatus(): Result<RegistrationQueueStatus>

    suspend fun searchUsernames(query: String): Result<List<FoundUser>>

    suspend fun getVerifier(): Result<String>
}

class RealUsernameRepository @Inject constructor(
    private val api: UsernameApi,
    private val usernamesChainProvider: UsernamesChainProvider
) : UsernameRepository {
    override suspend fun getVerifier(): Result<String> = runCancellableCatching {
        api.getAttester().attester
    }.mapError(Throwable::toBackendRequestError)

    override suspend fun checkUsernameAvailable(username: Username): Result<UsernameAvailabilityState> = runCancellableCatching {
        val usernameString = username.getDisplayUsername()
        val response = api.checkUsername(UsernameAvailableRequest(listOf(usernameString)))
        val entry = response.value[usernameString]
        // API returns digits as integers (e.g. 1, 42) — pad to 2-digit strings ("01", "42")
        // to match the preferredDigits format expected by the claim endpoint.
        val availableDigits = entry?.availableDigits
            ?.map { it.toString().padStart(2, '0') }
            .orEmpty()
        UsernameAvailabilityState.fromStatusAndDigits(entry?.status, availableDigits)
    }
        .flatten()
        .mapError(Throwable::toBackendRequestError)

    override suspend fun searchUsernames(query: String): Result<List<FoundUser>> = runCatching {
        val chain = usernamesChainProvider.chain()
        val usernames = api.searchUsernames(query).usernames
        usernames
            .filterNot { it.accountId.isNullOrEmpty() }
            .mapNotNull { it.toDomain(chain) }
    }

    override suspend fun claimUsername(params: ClaimUsernameParams): Result<UsernameClaimResult> = runCancellableCatching {
        val request = UsernameClaimRequest(
            candidateAccountId = params.candidateAddress,
            username = params.username,
            candidateSignature = params.candidateSignature.toHexString(true),
            identifierKey = params.identifierKey.value.toHexString(true),
            ringVrfKey = params.ringVrfKey.toHexString(true),
            proofOfOwnership = params.membershipSignature.toHexString(true),
            consumerRegistrationSignature = params.consumerSignature.toHexString(true),
            preferredDigits = params.preferredDigits.ifEmpty { null },
            dotns = UsernameClaimDotNsRequest(
                signature = params.dotNsSignature.toHexString(true),
                signedAt = params.dotNsSignedAt,
                reservedUsername = params.dotNsReservedUsername
            )
        )

        api.claimUsername(request).toClaimResult()
    }.mapError(::mapClaimUsernameError)

    override suspend fun getRegistrationQueueStatus(): Result<RegistrationQueueStatus> = runCatching {
        val response = api.getRegistrationQueueStatus()
        RegistrationQueueStatus.InQueue(
            position = response.queuePosition,
            group = response.group,
            estimatedIterationsRemaining = response.estimatedIterationsRemaining
        )
    }.flatRecover { error ->
        when {
            error.isHttpCode(HTTP_NOT_FOUND) -> Result.success(RegistrationQueueStatus.NotQueued)
            error.isHttpCode(HTTP_TOO_MANY_REQUESTS) -> Result.failure(
                QueueRateLimitedException((error as HttpException).retryAfterSeconds())
            )

            else -> Result.failure(error)
        }
    }

    private fun UsernameClaimResponse.toClaimResult(): UsernameClaimResult {
        return when (registrationOutcome) {
            RegistrationOutcome.PAYMENT_REQUIRED -> UsernameClaimResult.PaymentRequired
            RegistrationOutcome.QUEUED -> UsernameClaimResult.Queued(Username.fromFullValue(requireNotNull(username)))
            null -> UsernameClaimResult.Registered(Username.fromFullValue(requireNotNull(username)))
        }
    }

    private fun Throwable.isHttpCode(code: Int): Boolean {
        return this is HttpException && code() == code
    }

    private fun HttpException.retryAfterSeconds(): Long? {
        return response()?.headers()?.get(RETRY_AFTER_HEADER)?.toLongOrNull()
    }

    private companion object {
        const val HTTP_NOT_FOUND = 404
        const val HTTP_TOO_MANY_REQUESTS = 429
        const val RETRY_AFTER_HEADER = "Retry-After"
    }
}
