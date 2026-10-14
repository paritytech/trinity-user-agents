package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.feature_products_api.domain.ProductAccountIdProvider
import io.paritytech.polkadotapp.feature_products_api.domain.ProductRequestAccountResolver
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.CreateProofError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.GetAliasError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ListRingVrfKeysError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisterRingVrfKeyError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfSignError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRouter
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoHandledRequestRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.SsoSessionManager
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse.Companion.responseWith
import io.paritytech.polkadotapp.feature_sso_impl.domain.signTransaction.SsoSigningContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.onEach
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SsoService @Inject constructor(
    private val ssoSessionManager: SsoSessionManager,
    private val signingRouter: SigningRouter,
    private val signingContextHolder: SigningContextHolder,
    private val accountsProtocol: AccountsProtocol,
    private val ssoHandledRequestRepository: SsoHandledRequestRepository,
    private val productRequestAccountResolver: ProductRequestAccountResolver,
    private val productAccountIdProvider: ProductAccountIdProvider,
) {
    fun watchSsoEvents(): Flow<SsoSessionRequest> {
        return flowOfAll {
            ssoSessionManager.init()

            ssoSessionManager.allMessages
        }
            .onEach { request -> handleRequest(request) }
    }

    suspend fun disconnectSession(sessionId: SsoSessionId) {
        ssoSessionManager.disconnectSession(sessionId)
    }

    suspend fun sendResponse(response: SsoSessionResponse): Result<Unit> {
        val session = ssoSessionManager.awaitSession(response.sessionId)
        return session.sendResponseAndAwaitSent(response)
    }

    // CommunicationSession may re-deliver requests that the other side never cleaned up, so we
    // de-duplicate by request id (persisted across restarts) and process each one at most once.
    private suspend fun handleRequest(request: SsoSessionRequest) {
        if (ssoHandledRequestRepository.wasHandled(request)) {
            Timber.d("Skipping already handled SSO request ${request.requestId}")
            return
        }

        val session = ssoSessionManager.getSession(request.sessionId)
        if (session == null) {
            Timber.w("Session id ${request.sessionId} was not found")
            return
        }

        val sessionName = session.sessionData.name

        when (val content = request.content) {
            is SsoSessionRequest.Content.Disconnected -> handleDisconnected(request, sessionName)
            is SsoSessionRequest.Content.SigningRequest -> handleSigningRequest(request, content.request, session.sessionData, sessionName)
            is SsoSessionRequest.Content.CreateTransactionRequest -> handleSigningRequest(request, content.request, session.sessionData, sessionName)
            is SsoSessionRequest.Content.CreateTransactionLegacyRequest -> handleLegacySigningRequest(request, content.request, session.sessionData, sessionName)
            is SsoSessionRequest.Content.SignRawLegacyRequest -> handleLegacySigningRequest(request, content.request, session.sessionData, sessionName)
            is SsoSessionRequest.Content.AliasRequest -> handleAliasRequest(request, content, sessionName)
            is SsoSessionRequest.Content.RegisterRingVrfKeyRequest -> handleRegisterRingVrfKeyRequest(request, content, sessionName)
            is SsoSessionRequest.Content.ListRingVrfKeysRequest -> handleListRingVrfKeysRequest(request, content, sessionName)
            is SsoSessionRequest.Content.RingVrfSignRequest -> handleRingVrfSignRequest(request, content, sessionName)
            is SsoSessionRequest.Content.CreateProofRequest -> handleCreateProofRequest(request, content, sessionName)
            is SsoSessionRequest.Content.SignVrfRequest -> handleSignVrfRequest(request, content, sessionName)
            is SsoSessionRequest.Content.ResourceAllocationRequest -> handleResourceAllocationRequest(request, content, sessionName)
            is SsoSessionRequest.Content.ProductSubtreeRequest -> handleProductSubtreeRequest(request, content, sessionName)
        }

        ssoHandledRequestRepository.markHandled(request)
    }

    private suspend fun handleDisconnected(request: SsoSessionRequest, sessionName: String) {
        Timber.d("Disconnected received from $sessionName")
        ssoSessionManager.deleteSession(request.sessionId)
    }

    private suspend fun handleSigningRequest(
        request: SsoSessionRequest,
        signingRequest: SigningRequestBody.ProductAccountSigning,
        sessionData: SsoSessionData,
        sessionName: String,
    ) {
        Timber.d("SigningRequest received from $sessionName")

        openSigningScreen(
            request = request,
            signingRequest = signingRequest,
            signingAccount = SigningAccount.Product(signingRequest.account),
            sessionData = sessionData
        )
    }

    /**
     * Legacy signing requests carry a plain account id. Reverse-resolve it before showing any UI;
     * if it isn't an account we control, reject the request without opening the signing screen.
     */
    private suspend fun handleLegacySigningRequest(
        request: SsoSessionRequest,
        signingRequest: SigningRequestBody.LegacyAccountSigning,
        sessionData: SsoSessionData,
        sessionName: String,
    ) {
        productRequestAccountResolver.resolve(signingRequest.account)
            .onSuccess { signingAccount -> openSigningScreen(request, signingRequest, signingAccount, sessionData) }
            .onFailure { error ->
                Timber.w(error, "Rejecting legacy signing request from $sessionName: account not resolved")

                val message = error.message ?: "Unknown legacy account"
                val responseContent = when (signingRequest) {
                    is SigningRequestBody.RawLegacy -> SsoSessionResponse.Content.FailedToSignRawLegacy(message)
                    is SigningRequestBody.TransactionLegacy -> SsoSessionResponse.Content.FailedToSignTransaction(message)
                    is SigningRequestBody.CreateTransactionLegacy -> SsoSessionResponse.Content.FailedToCreateTransaction(message)
                }
                sendResponse(request.responseWith(responseContent))
            }
    }

    private suspend fun openSigningScreen(
        request: SsoSessionRequest,
        signingRequest: SigningRequestBody,
        signingAccount: SigningAccount,
        sessionData: SsoSessionData,
    ) {
        val signingContext = SsoSigningContext(
            sessionData = sessionData,
            request = request,
            ssoService = this,
            signingRequestBody = signingRequest,
            signingAccount = signingAccount,
        )

        signingContextHolder.set(signingContext)
        signingRouter.openSignTransaction()
    }

    private suspend fun handleAliasRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.AliasRequest,
        sessionName: String,
    ) {
        Timber.d("AliasRequest received from $sessionName")

        val responseContent = accountsProtocol.getContextualAlias(content.callingProduct, content.keyHandle, content.context, content.ring).fold(
            onSuccess = { alias -> SsoSessionResponse.Content.AliasResult(alias) },
            onFailure = { error ->
                val getAliasError = error as? GetAliasError ?: GetAliasError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToGetAlias(getAliasError)
            }
        )

        val response = request.responseWith(responseContent)
        sendResponse(response)
    }

    private suspend fun handleRegisterRingVrfKeyRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.RegisterRingVrfKeyRequest,
        sessionName: String,
    ) {
        Timber.d("RegisterRingVrfKeyRequest received from $sessionName")

        val responseContent = accountsProtocol.registerRingVrfKey(content.callingProduct, content.index, content.ring).fold(
            onSuccess = { publicKey -> SsoSessionResponse.Content.RegisterRingVrfKeyResult(publicKey) },
            onFailure = { error ->
                val registerError = error as? RegisterRingVrfKeyError
                    ?: RegisterRingVrfKeyError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToRegisterRingVrfKey(registerError)
            }
        )

        sendResponse(request.responseWith(responseContent))
    }

    private suspend fun handleListRingVrfKeysRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.ListRingVrfKeysRequest,
        sessionName: String,
    ) {
        Timber.d("ListRingVrfKeysRequest received from $sessionName")

        val responseContent = accountsProtocol.listRingVrfKeys(content.callingProduct, content.owner, content.disclosure).fold(
            onSuccess = { entries -> SsoSessionResponse.Content.ListRingVrfKeysResult(entries) },
            onFailure = { error ->
                val listError = error as? ListRingVrfKeysError
                    ?: ListRingVrfKeysError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToListRingVrfKeys(listError)
            }
        )

        sendResponse(request.responseWith(responseContent))
    }

    private suspend fun handleRingVrfSignRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.RingVrfSignRequest,
        sessionName: String,
    ) {
        Timber.d("RingVrfSignRequest received from $sessionName")

        val responseContent = accountsProtocol.ringVrfSign(content.callingProduct, content.keyHandle, content.message).fold(
            onSuccess = { signature -> SsoSessionResponse.Content.RingVrfSignResult(signature.toDataByteArray()) },
            onFailure = { error ->
                val signError = error as? RingVrfSignError ?: RingVrfSignError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToRingVrfSign(signError)
            }
        )

        sendResponse(request.responseWith(responseContent))
    }

    private suspend fun handleCreateProofRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.CreateProofRequest,
        sessionName: String,
    ) {
        Timber.d("CreateProofRequest received from $sessionName")

        val responseContent = accountsProtocol.createProof(content.callingProduct, content.keyHandle, content.context, content.ring, content.message).fold(
            onSuccess = { proof -> SsoSessionResponse.Content.ProofResult(proof) },
            onFailure = { error ->
                val createProofError = error as? CreateProofError ?: CreateProofError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToCreateProof(createProofError)
            }
        )

        val response = request.responseWith(responseContent)
        sendResponse(response)
    }

    private suspend fun handleSignVrfRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.SignVrfRequest,
        sessionName: String,
    ) {
        Timber.d("SignVrfRequest received from $sessionName")

        val responseContent = accountsProtocol.signVrf(
            content.callingProduct,
            content.account,
            content.transcriptLabel,
            content.items,
        ).fold(
            onSuccess = { signature -> SsoSessionResponse.Content.SignVrfResult(signature) },
            onFailure = { error ->
                val signVrfError = error as? SignVrfError ?: SignVrfError.Unknown(error.message ?: "Unknown error")
                SsoSessionResponse.Content.FailedToSignVrf(signVrfError)
            }
        )

        val response = request.responseWith(responseContent)
        sendResponse(response)
    }

    // RFC-0022: consent-free — the product subtree public key is not secret material, and the
    // product's individual accounts become public on-chain the moment they are used.
    private suspend fun handleProductSubtreeRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.ProductSubtreeRequest,
        sessionName: String,
    ) {
        Timber.d("ProductSubtreeRequest received from $sessionName for ${content.productId}")

        val responseContent = productAccountIdProvider.deriveProductSubtreePublicKey(content.productId).fold(
            onSuccess = { publicKey -> SsoSessionResponse.Content.ProductSubtreeResult(publicKey) },
            onFailure = { error -> SsoSessionResponse.Content.FailedToGetProductSubtree(error.message ?: "Unknown error") },
        )

        val response = request.responseWith(responseContent)
        sendResponse(response)
    }

    private suspend fun handleResourceAllocationRequest(
        request: SsoSessionRequest,
        content: SsoSessionRequest.Content.ResourceAllocationRequest,
        sessionName: String,
    ) {
        Timber.d("ResourceAllocationRequest received from $sessionName for ${content.callingProduct}")

        val responseContent = runCatching {
            accountsProtocol.requestResourceAllocation(
                callingProduct = content.callingProduct,
                resources = content.resources,
                onExisting = content.onExisting,
            )
        }.fold(
            onSuccess = { outcomes -> SsoSessionResponse.Content.ResourceAllocationResult(outcomes) },
            onFailure = { error -> SsoSessionResponse.Content.FailedToAllocateResources(error.message ?: "Unknown error") },
        )

        val response = request.responseWith(responseContent)
        sendResponse(response)
    }
}
