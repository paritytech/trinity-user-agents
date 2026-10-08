package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.errors.UserCancellation
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.SsoHandledRequestDao
import io.paritytech.polkadotapp.database.model.SsoHandledRequestLocal
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.ProductAccountIdProvider
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.RawPayloadContent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoHandledRequestRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.SsoCommunicationSession
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.SsoSessionManager
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private val SESSION = SsoSessionId("session")
private val CALLER = ProductId.fromStoredValue("lottery.dot")
private val OTHER = ProductId.fromStoredValue("voting.dot")
private const val NEXT = "next"

/**
 * A `Cancel` reaches a request while its prompt is up: the prompt's coroutine is cancelled, nothing is posted for it,
 * it is recorded as handled, and the requests behind it are served.
 */
class SsoServiceWithdrawalTest {
    private val incoming = Channel<SsoSessionRequest>(Channel.UNLIMITED)
    private val sent = mutableListOf<SsoSessionResponse>()
    private val handled = mutableSetOf<String>()

    private val session: SsoCommunicationSession = mockk {
        every { sessionData } returns mockk(relaxed = true)
        coEvery { sendResponseAndAwaitSent(any()) } answers {
            sent += firstArg<SsoSessionResponse>()
            Result.success(Unit)
        }
    }

    private val sessionManager: SsoSessionManager = mockk {
        coEvery { init() } just Runs
        every { allMessages } returns incoming.receiveAsFlow()
        every { getSession(any()) } returns session
        coEvery { awaitSession(any()) } returns session
    }

    private val handledDao = object : SsoHandledRequestDao {
        override suspend fun insert(local: SsoHandledRequestLocal) {
            handled += local.requestId
        }

        override suspend fun isHandled(sessionId: String, requestId: String) = requestId in handled
    }

    // Every prompt waits for an answer that never comes, so only a withdrawal can end it
    private val accountsProtocol: AccountsProtocol = mockk {
        coEvery { requestResourceAllocation(any(), any(), any()) } coAnswers { awaitCancellation() }
        coEvery { getContextualAlias(any(), any(), any(), any()) } coAnswers { awaitCancellation() }
        coEvery { createProof(any(), any(), any(), any(), any()) } coAnswers { awaitCancellation() }
    }

    private val productAccountIdProvider: ProductAccountIdProvider = mockk {
        coEvery { deriveProductSubtreePublicKey(any()) } returns Result.success(ByteArray(32).toDataByteArray())
    }

    private val signingContextHolder = SigningContextHolder()

    private val service = SsoService(
        ssoSessionManager = sessionManager,
        signingRouter = mockk(relaxed = true),
        signingContextHolder = signingContextHolder,
        accountsProtocol = accountsProtocol,
        ssoHandledRequestRepository = SsoHandledRequestRepository(handledDao),
        productRequestAccountResolver = mockk(),
        productAccountIdProvider = productAccountIdProvider,
    )

    private val keyHandle = ProductAccountId(OTHER.value, DerivationIndex32.default())
    private val proofContext = ProductProofContext(OTHER, DerivationIndex32.default())

    @Test
    fun `nothing is posted for a resource allocation when it is withdrawn while prompting`() = runTest {
        assertNothingPostedWhenWithdrawnWhilePrompting(allocation())
    }

    @Test
    fun `nothing is posted for an alias when it is withdrawn while prompting`() = runTest {
        assertNothingPostedWhenWithdrawnWhilePrompting(SsoSessionRequest.Content.AliasRequest(CALLER, keyHandle, proofContext, mockk()))
    }

    @Test
    fun `nothing is posted for a proof when it is withdrawn while prompting`() = runTest {
        assertNothingPostedWhenWithdrawnWhilePrompting(SsoSessionRequest.Content.CreateProofRequest(CALLER, keyHandle, proofContext, mockk(), byteArrayOf(1)))
    }

    @Test
    fun `a request never prompts when it is withdrawn before it arrives`() = runTest {
        start()

        receive(cancel("early"), request("early", allocation()), request(NEXT, subtree()))

        verifyAllocationPrompted(times = 0)
        assertHandled("early")
        assertAnswered(NEXT)
    }

    @Test
    fun `a withdrawn request does not prompt again when it is redelivered`() = runTest {
        start()
        receive(request("ra", allocation()))
        receive(cancel("ra"))

        receive(request("ra", allocation()))

        verifyAllocationPrompted(times = 1)
    }

    @Test
    fun `the signing sheet closes and a rejection posts nothing when the request is withdrawn`() = runTest {
        start()
        val sheet = openSigningSheet("sign")
        var closed = false
        launch { sheet.awaitWithdrawal().also { closed = true } }

        receive(cancel("sign"))
        sheet.deliverRejection()

        assertTrue(closed)
        assertAnswered()
    }

    @Test
    fun `a signing sheet is withdrawn when another signing request has taken the holder since`() = runTest {
        start()
        val first = openSigningSheet("sign-a")
        openSigningSheet("sign-b")
        var closed = false
        launch { first.awaitWithdrawal().also { closed = true } }

        receive(cancel("sign-a"))
        first.deliverRejection()

        assertTrue(closed)
        assertAnswered()
    }

    @Test
    fun `approving signs nothing and posts nothing when the request is withdrawn`() = runTest {
        start()
        val sheet = openSigningSheet("sign")
        receive(cancel("sign"))
        var signed = false

        val result = sheet.approve { Result.failure<SignedTransaction>(IllegalStateException()).also { signed = true } }

        assertFalse(signed)
        assertTrue("expected a silent cancellation but was ${result.exceptionOrNull()}", result.exceptionOrNull() is UserCancellation)
        assertAnswered()
    }

    private suspend fun TestScope.assertNothingPostedWhenWithdrawnWhilePrompting(content: SsoSessionRequest.Content) {
        start()
        receive(request("prompting", content), request(NEXT, subtree()))
        assertAnswered()

        receive(cancel("prompting"))

        assertAnswered(NEXT)
        assertHandled("prompting")
    }

    private fun TestScope.openSigningSheet(id: String): SigningContext {
        val body = SigningRequestBody.Raw(SigningRawPayload(keyHandle, RawPayloadContent.Bytes(byteArrayOf(1))))
        receive(request(id, SsoSessionRequest.Content.SigningRequest(body)))
        return requireNotNull(signingContextHolder.get())
    }

    private fun verifyAllocationPrompted(times: Int) {
        coVerify(exactly = times) { accountsProtocol.requestResourceAllocation(any(), any(), any()) }
    }

    private fun assertAnswered(vararg requestIds: String) = assertEquals(requestIds.toList(), sent.map { it.respondingTo })

    private fun assertHandled(requestId: String) = assertTrue("$requestId was not recorded as handled", requestId in handled)

    private fun TestScope.start() {
        backgroundScope.launch { service.watchSsoEvents().collect() }
        runCurrent()
    }

    private fun TestScope.receive(vararg requests: SsoSessionRequest) {
        requests.forEach { incoming.trySend(it) }
        runCurrent()
    }

    private fun subtree() = SsoSessionRequest.Content.ProductSubtreeRequest(OTHER)

    private fun allocation() =
        SsoSessionRequest.Content.ResourceAllocationRequest(CALLER, listOf(ApAllocatableResource.BulletInAllowance), OnExistingAllowancePolicy.IGNORE)

    private fun request(id: String, content: SsoSessionRequest.Content) = SsoSessionRequest(SESSION, id, content)

    private fun cancel(target: String) = request("cancel-$target", SsoSessionRequest.Content.Cancel(target))
}
