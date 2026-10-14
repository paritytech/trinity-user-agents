package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.types.BSResult
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse.Companion.responseWith
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private val SESSION_ID = SsoSessionId("session")
private const val REQUEST_ID = "request"

class SsoProductSubtreeMessageTest {
    @Test
    fun `request round trips through the wire`() {
        val request = productSubtreeRequest()

        val decoded = request.toEncodedMessage().toSsoSessionRequest(SESSION_ID, requireNotNull(DotNsTld.parse("dot"))).getOrThrow()

        val content = decoded.content as SsoSessionRequest.Content.ProductSubtreeRequest
        assertEquals("browse.dot", content.productId.value)
    }

    @Test
    fun `successful response carries the product public key`() {
        val publicKey = ByteArray(32) { it.toByte() }.toDataByteArray()

        val content = decodeResponseContent(SsoSessionResponse.Content.ProductSubtreeResult(publicKey))

        assertTrue(content.productPublicKey is BSResult.Ok)
        assertArrayEquals(publicKey.value, (content.productPublicKey as BSResult.Ok).value.value)
    }

    @Test
    fun `failed response carries the error message`() {
        val content = decodeResponseContent(SsoSessionResponse.Content.FailedToGetProductSubtree("boom"))

        assertTrue(content.productPublicKey is BSResult.Err)
        assertEquals("boom", (content.productPublicKey as BSResult.Err).error)
    }

    /**
     * Enum indices are the cross-host wire contract: a shift here silently reinterprets every
     * message a peer sends.
     */
    @Test
    fun `product subtree variants sit at wire indices 16 and 17`() {
        assertEquals(16, variantIndexOf(SsoMessageContent.ProductSubtreeRequest("browse.dot")))

        val response = SsoMessageContent.ProductSubtreeResponse(REQUEST_ID, BSResult.Err("e"))
        assertEquals(17, variantIndexOf(response))
    }

    private fun productSubtreeRequest() = SsoSessionRequest(
        sessionId = SESSION_ID,
        requestId = REQUEST_ID,
        content = SsoSessionRequest.Content.ProductSubtreeRequest(ProductId.fromStoredValue("browse.dot")),
    )

    private fun decodeResponseContent(content: SsoSessionResponse.Content): SsoMessageContent.ProductSubtreeResponse {
        val encoded = productSubtreeRequest().responseWith(content).toEncodedMessage()

        val message = encoded.decodeSsoSessionMessage().getOrThrow()
        val versioned = message.versioned as VersionedSsoSessionMessage.V1

        return versioned.message.content as SsoMessageContent.ProductSubtreeResponse
    }

    private fun variantIndexOf(content: SsoMessageContent): Int {
        return BinaryScale.encodeToByteArray(SsoSessionMessageV1(content)).first().toInt()
    }
}
