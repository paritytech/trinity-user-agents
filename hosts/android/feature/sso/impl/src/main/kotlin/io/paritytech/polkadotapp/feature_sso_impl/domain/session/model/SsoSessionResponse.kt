package io.paritytech.polkadotapp.feature_sso_impl.domain.session.model

import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.CreateProofError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.GetAliasError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ListRingVrfKeysError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisterRingVrfKeyError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfSignError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import java.util.UUID

class SsoSessionResponse private constructor(
    val sessionId: SsoSessionId,
    val respondingTo: SsoSessionRequestId,
    val ownRequestId: SsoSessionRequestId,
    val content: Content
) {
    companion object {
        fun SsoSessionRequest.responseWith(content: Content): SsoSessionResponse {
            return SsoSessionResponse(
                sessionId = sessionId,
                respondingTo = requestId,
                ownRequestId = UUID.randomUUID().toString(),
                content = content
            )
        }
    }

    sealed class Content {
        class SignedPayload(val signed: SignedTransaction.WithDedicatedSignature) : Content()

        class FailedToSignTransaction(val error: String) : Content()

        class SignedGeneralTransaction(val signedTx: DataByteArray) : Content()

        class FailedToCreateTransaction(val error: String) : Content()

        class SignedRawLegacy(val signature: DataByteArray) : Content()

        class FailedToSignRawLegacy(val error: String) : Content()

        class AliasResult(val alias: ContextualAlias) : Content()

        class FailedToGetAlias(val error: GetAliasError) : Content()

        class ProofResult(val proof: RingVrfProof) : Content()

        class FailedToCreateProof(val error: CreateProofError) : Content()

        class SignVrfResult(val signature: VrfSignature) : Content()

        class FailedToSignVrf(val error: SignVrfError) : Content()

        class RegisterRingVrfKeyResult(val publicKey: DataByteArray) : Content()

        class FailedToRegisterRingVrfKey(val error: RegisterRingVrfKeyError) : Content()

        class ListRingVrfKeysResult(val entries: List<RegisteredRingVrfKey>) : Content()

        class FailedToListRingVrfKeys(val error: ListRingVrfKeysError) : Content()

        class RingVrfSignResult(val signature: DataByteArray) : Content()

        class FailedToRingVrfSign(val error: RingVrfSignError) : Content()

        class ResourceAllocationResult(val outcomes: List<ApAllocationOutcome>) : Content()

        class FailedToAllocateResources(val error: String) : Content()

        class ProductSubtreeResult(val productPublicKey: EncodedPublicKey) : Content()

        class FailedToGetProductSubtree(val error: String) : Content()
    }
}
