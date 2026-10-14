package io.paritytech.polkadotapp.feature_sso_impl.domain.signTransaction

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_sso_impl.domain.SsoService
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse.Companion.responseWith
import timber.log.Timber

class SsoSigningContext(
    sessionData: SsoSessionData,
    private val request: SsoSessionRequest,
    private val ssoService: SsoService,
    override val signingRequestBody: SigningRequestBody,
    override val signingAccount: SigningAccount,
) : SigningContext {
    override val requesterName: String = sessionData.name
    override val requesterIconUrl: String = sessionData.icon

    override suspend fun approve(sign: suspend () -> Result<SignedTransaction>): Result<Unit> =
        sign().flatMap { signedTransaction -> deliverSignedResult(signedTransaction) }

    private suspend fun deliverSignedResult(signedTransaction: SignedTransaction): Result<Unit> {
        Timber.d("Delivering signed result to $requesterName")
        val responseContent = when (signedTransaction) {
            is SignedTransaction.GeneralTransaction -> SsoSessionResponse.Content.SignedGeneralTransaction(signedTransaction.signedTx)
            is SignedTransaction.Raw -> if (signingRequestBody is SigningRequestBody.RawLegacy) {
                SsoSessionResponse.Content.SignedRawLegacy(signedTransaction.signature)
            } else {
                SsoSessionResponse.Content.SignedPayload(signedTransaction)
            }
            is SignedTransaction.PayloadJson -> SsoSessionResponse.Content.SignedPayload(signedTransaction)
            // Unreachable: SignVrfRequest is served by AccountsProtocol, which owns its own context.
            is SignedTransaction.Vrf -> return Result.failure(
                IllegalStateException("VRF results are delivered via AccountsProtocol, not SsoSigningContext")
            )
        }
        val response = request.responseWith(responseContent)
        return ssoService.sendResponse(response)
            .onSuccess { Timber.d("Signed result delivered to $requesterName") }
            .onFailure { Timber.e(it, "Failed to deliver signed result to $requesterName") }
    }

    override suspend fun deliverRejection(): Result<Unit> {
        Timber.d("Delivering rejection to $requesterName")
        val responseContent = when (signingRequestBody) {
            is SigningRequestBody.Transaction, is SigningRequestBody.Raw -> SsoSessionResponse.Content.FailedToSignTransaction("Rejected")
            is SigningRequestBody.CreateTransaction -> SsoSessionResponse.Content.FailedToCreateTransaction("Rejected")
            is SigningRequestBody.RawLegacy -> SsoSessionResponse.Content.FailedToSignRawLegacy("Rejected")
            is SigningRequestBody.TransactionLegacy -> SsoSessionResponse.Content.FailedToSignTransaction("Rejected")
            is SigningRequestBody.CreateTransactionLegacy -> SsoSessionResponse.Content.FailedToCreateTransaction("Rejected")
            // Unreachable, see deliverSignedResult.
            is SigningRequestBody.SignVrf -> return Result.failure(
                IllegalStateException("VRF rejections are delivered via AccountsProtocol, not SsoSigningContext")
            )
        }
        val response = request.responseWith(responseContent)
        return ssoService.sendResponse(response)
            .onSuccess { Timber.d("Rejection delivered to $requesterName") }
            .onFailure { Timber.e(it, "Failed to deliver rejection to $requesterName") }
    }
}
