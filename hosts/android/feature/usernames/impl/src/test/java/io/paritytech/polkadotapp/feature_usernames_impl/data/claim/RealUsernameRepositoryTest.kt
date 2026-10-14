package io.paritytech.polkadotapp.feature_usernames_impl.data.claim

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.UsernameApi
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimResponse
import io.paritytech.polkadotapp.feature_usernames_impl.domain.UsernamesChainProvider
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameParams
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameClaimResult
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import kotlinx.coroutines.runBlocking
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import retrofit2.HttpException
import retrofit2.Response

class RealUsernameRepositoryTest {
    private val api: UsernameApi = mock(UsernameApi::class.java)
    private val chainProvider: UsernamesChainProvider = mock(UsernamesChainProvider::class.java)

    private val repository = RealUsernameRepository(api, chainProvider)

    private val params = ClaimUsernameParams(
        username = "alice.07",
        preferredDigits = "07",
        candidateAddress = "5FakeAddress",
        candidateSignature = ByteArray(64),
        consumerSignature = ByteArray(64),
        membershipSignature = ByteArray(64),
        ringVrfKey = ByteArray(32),
        identifierKey = ByteArray(65).toDataByteArray(),
        dotNsSignature = ByteArray(64),
        dotNsSignedAt = 0L,
        dotNsReservedUsername = "alice"
    )

    private val requests = mutableListOf<UsernameClaimRequest>()

    @Test
    fun `claim maps a registered response`() = runBlocking<Unit> {
        withClaimResponse(registeredResponse())

        val result = repository.claimUsername(params)

        assertSuccess(result)
        assertEquals(UsernameClaimResult.Registered::class, result.getOrNull()!!::class)
        val request = requests.single()
        assertEquals("alice.07", request.username)
        assertEquals("07", request.preferredDigits)
        assertEquals("alice", request.dotns?.reservedUsername)
    }

    @Test
    fun `claim maps conflict to already claimed`() = runBlocking<Unit> {
        withClaimFailure(httpException(code = 409, body = "{}"))

        val result = repository.claimUsername(params)

        assertAlreadyClaimed(result)
        assertEquals(1, requests.size)
    }

    @Test
    fun `claim maps other backend failures`() = runBlocking<Unit> {
        withClaimFailure(httpException(code = 403, body = "{}"))

        val result = repository.claimUsername(params)

        assertBackendFailure(result, BackendRequestError.Server(403))
        assertEquals(1, requests.size)
    }

    private suspend fun withClaimResponse(response: UsernameClaimResponse) {
        whenever(api.claimUsername(any())).thenAnswer { invocation ->
            requests += invocation.getArgument<UsernameClaimRequest>(0)
            response
        }
    }

    private suspend fun withClaimFailure(error: Throwable) {
        whenever(api.claimUsername(any())).thenAnswer { invocation ->
            requests += invocation.getArgument<UsernameClaimRequest>(0)
            throw error
        }
    }

    private fun registeredResponse() = UsernameClaimResponse(
        baseUsername = "alice",
        digits = "07",
        username = "alice.07",
        registrationOutcome = null
    )

    private fun httpException(code: Int, body: String): HttpException {
        val errorBody = body.toResponseBody("application/json".toMediaType())
        return HttpException(Response.error<UsernameClaimResponse>(code, errorBody))
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private fun assertAlreadyClaimed(result: Result<*>) {
        assertTrue(
            "expected UsernameAlreadyClaimedException but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is UsernameAlreadyClaimedException,
        )
    }

    private fun assertBackendFailure(result: Result<*>, expected: BackendRequestError) {
        assertEquals(expected, result.exceptionOrNull())
    }
}
