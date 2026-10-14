package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.WithLength32
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.types.BSResult
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductDerivationIndexScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductIdScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.RingLocationScale
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequestId
import kotlinx.serialization.Serializable

/**
 * Union of SSO message types that can be exchanged between wallet and host.
 */
@Serializable
sealed class SsoMessageContent {
    @Serializable
    @EnumIndex(0)
    data object Disconnected : SsoMessageContent()

    @Serializable
    @EnumIndex(1)
    class SigningRequest(val request: SsoSigningRequestScale) : SsoMessageContent()

    @Serializable
    @EnumIndex(2)
    class SigningResponse(
        val respondingTo: SsoSessionRequestId,
        val signedPayload: BSResult<SsoSignedPayloadJsonScale, String>
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(3)
    class RingVrfAliasRequest(
        val callingProductId: ProductIdScale,
        val keyHandle: ProductAccountIdScale,
        val context: ProductProofContextScale,
        val ring: RingLocationScale,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(4)
    class RingVrfAliasResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<SsoContextualAliasScale, SsoRingVrfErrorScale>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(5)
    class ResourceAllocationRequest(
        val request: SsoResourceAllocationRequestScale,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(6)
    class ResourceAllocationResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<List<SsoApAllocationOutcomeScale>, String>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(7)
    class CreateTransactionRequest(val request: SsoCreateTransactionRequestScale) : SsoMessageContent()

    @Serializable
    @EnumIndex(8)
    class CreateTransactionResponse(
        val respondingTo: SsoSessionRequestId,
        val signedTx: BSResult<DataByteArray, String>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(9)
    class CreateTransactionLegacyRequest(val request: SsoCreateTransactionLegacyRequestScale) : SsoMessageContent()

    @Serializable
    @EnumIndex(10)
    class SignRawLegacyRequest(val request: SsoSignRawLegacyRequestScale) : SsoMessageContent()

    @Serializable
    @EnumIndex(11)
    class SignRawLegacyResponse(
        val respondingTo: SsoSessionRequestId,
        val signature: BSResult<DataByteArray, String>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(12)
    class RingVrfProofRequest(
        val callingProductId: ProductIdScale,
        val keyHandle: ProductAccountIdScale,
        val context: ProductProofContextScale,
        val ring: RingLocationScale,
        val message: DataByteArray,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(13)
    class RingVrfProofResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<SsoRingVrfProofScale, SsoRingVrfErrorScale>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(14)
    class SignVrfRequest(
        val callingProductId: ProductIdScale,
        val account: ProductAccountIdScale,
        val transcriptLabel: DataByteArray,
        val items: List<VrfTranscriptItemScale>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(15)
    class SignVrfResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<VrfSignatureScale, SsoSignVrfErrorScale>,
    ) : SsoMessageContent()

    /**
     * RFC-0022: asks the Account Holder for `//product//{productId}`'s public key. Consent-free —
     * the response carries no secret material, and product accounts become public on-chain once used.
     */
    @Serializable
    @EnumIndex(16)
    class ProductSubtreeRequest(val productId: ProductIdScale) : SsoMessageContent()

    @Serializable
    @EnumIndex(17)
    class ProductSubtreeResponse(
        val respondingTo: SsoSessionRequestId,
        val productPublicKey: BSResult<WithLength32<ByteArray>, String>,
    ) : SsoMessageContent()

    // RFC-0024 key management. The Account Holder is the authoritative registry: registration always
    // reaches it, while a Host holding a current snapshot may answer `list` locally.
    @Serializable
    @EnumIndex(18)
    class RegisterRingVrfKeyRequest(
        val callingProductId: ProductIdScale,
        val index: ProductDerivationIndexScale,
        val ring: RingLocationScale,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(19)
    class RegisterRingVrfKeyResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<WithLength32<ByteArray>, SsoRingVrfErrorScale>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(20)
    class ListRingVrfKeysRequest(
        val callingProductId: ProductIdScale,
        val owner: ProductIdScale,
        val disclosure: RingVrfKeyDisclosureScale,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(21)
    class ListRingVrfKeysResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<List<RegisteredRingVrfKeyScale>, SsoRingVrfErrorScale>,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(22)
    class RingVrfSignRequest(
        val callingProductId: ProductIdScale,
        val keyHandle: ProductAccountIdScale,
        val message: DataByteArray,
    ) : SsoMessageContent()

    @Serializable
    @EnumIndex(23)
    class RingVrfSignResponse(
        val respondingTo: SsoSessionRequestId,
        val payload: BSResult<DataByteArray, SsoRingVrfErrorScale>,
    ) : SsoMessageContent()
}
