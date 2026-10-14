package io.paritytech.polkadotapp.feature_sso_impl.domain.session.model

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody

typealias SsoSessionRequestId = String

class SsoSessionRequest(
    val sessionId: SsoSessionId,
    val requestId: SsoSessionRequestId,
    val content: Content
) {
    sealed class Content {
        data object Disconnected : Content()

        class SigningRequest(val request: SigningRequestBody.ResultHasSignature) : Content()

        class CreateTransactionRequest(val request: SigningRequestBody.CreateTransaction) : Content()

        class CreateTransactionLegacyRequest(val request: SigningRequestBody.CreateTransactionLegacy) : Content()

        class SignRawLegacyRequest(val request: SigningRequestBody.RawLegacy) : Content()

        class AliasRequest(
            val callingProduct: ProductId,
            val keyHandle: ProductAccountId,
            val context: ProductProofContext,
            val ring: RingLocation,
        ) : Content()

        class CreateProofRequest(
            val callingProduct: ProductId,
            val keyHandle: ProductAccountId,
            val context: ProductProofContext,
            val ring: RingLocation,
            val message: ByteArray,
        ) : Content()

        class RegisterRingVrfKeyRequest(
            val callingProduct: ProductId,
            val index: DerivationIndex32,
            val ring: RingLocation,
        ) : Content()

        class ListRingVrfKeysRequest(
            val callingProduct: ProductId,
            val owner: ProductId,
            val disclosure: RingVrfKeyDisclosure,
        ) : Content()

        class RingVrfSignRequest(
            val callingProduct: ProductId,
            val keyHandle: ProductAccountId,
            val message: ByteArray,
        ) : Content()

        class SignVrfRequest(
            val callingProduct: ProductId,
            val account: ProductAccountId,
            val transcriptLabel: ByteArray,
            val items: List<VrfTranscriptItem>,
        ) : Content()

        class ResourceAllocationRequest(
            val callingProduct: ProductId,
            val resources: List<ApAllocatableResource>,
            val onExisting: OnExistingAllowancePolicy,
        ) : Content()

        class ProductSubtreeRequest(val productId: ProductId) : Content()
    }
}
