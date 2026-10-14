package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.types.BSResult
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

private val SESSION_ID = SsoSessionId("session")
private val TLD = requireNotNull(DotNsTld.parse("dot"))
private const val REQUEST_ID = "request"

private val CALLER = ProductId.fromStoredValue("game.dot")
private val KEY_HANDLE = ProductAccountId("peopl.dot", DerivationIndex32.fromUInt(0u))

private val RING = RingLocation(
    chainId = ByteArray(32) { 7 }.toDataByteArray(),
    junctions = listOf(
        RingLocationJunction.PalletInstance(9u),
        RingLocationJunction.CollectionId(RingCollectionId.paddedString("pop:polkadot.network/people").value),
    )
)

/**
 * RFC-0024 wire contract. Enum indices are the cross-host agreement: a shift here silently
 * reinterprets every message a peer sends.
 */
class SsoRingVrfKeyMessageTest {
    @Test
    fun `register request round trips through the wire`() {
        val request = requestWith(
            SsoSessionRequest.Content.RegisterRingVrfKeyRequest(
                callingProduct = CALLER,
                index = DerivationIndex32.fromUInt(4u),
                ring = RING,
            )
        )

        val decoded = request.toEncodedMessage().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()

        val content = decoded.content as SsoSessionRequest.Content.RegisterRingVrfKeyRequest
        assertEquals(CALLER, content.callingProduct)
        assertEquals(DerivationIndex32.fromUInt(4u), content.index)
        assertEquals(RING, content.ring)
    }

    @Test
    fun `list request round trips with its disclosure`() {
        val request = requestWith(
            SsoSessionRequest.Content.ListRingVrfKeysRequest(
                callingProduct = CALLER,
                owner = ProductId.fromStoredValue("peopl.dot"),
                disclosure = RingVrfKeyDisclosure.PUBLIC_KEY,
            )
        )

        val decoded = request.toEncodedMessage().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()

        val content = decoded.content as SsoSessionRequest.Content.ListRingVrfKeysRequest
        assertEquals("peopl.dot", content.owner.value)
        assertEquals(RingVrfKeyDisclosure.PUBLIC_KEY, content.disclosure)
    }

    @Test
    fun `sign request round trips the handle and message`() {
        val message = ByteArray(16) { it.toByte() }
        val request = requestWith(
            SsoSessionRequest.Content.RingVrfSignRequest(
                callingProduct = CALLER,
                keyHandle = KEY_HANDLE,
                message = message,
            )
        )

        val decoded = request.toEncodedMessage().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()

        val content = decoded.content as SsoSessionRequest.Content.RingVrfSignRequest
        assertEquals(KEY_HANDLE, content.keyHandle)
        assertArrayEquals(message, content.message)
    }

    @Test
    fun `alias and proof requests carry the key handle`() {
        val context = ProductProofContext(CALLER, DerivationIndex32.fromUInt(1u))

        val alias = requestWith(
            SsoSessionRequest.Content.AliasRequest(CALLER, KEY_HANDLE, context, RING)
        ).toEncodedMessage().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()
        assertEquals(KEY_HANDLE, (alias.content as SsoSessionRequest.Content.AliasRequest).keyHandle)

        val proof = requestWith(
            SsoSessionRequest.Content.CreateProofRequest(CALLER, KEY_HANDLE, context, RING, ByteArray(4))
        ).toEncodedMessage().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()
        assertEquals(KEY_HANDLE, (proof.content as SsoSessionRequest.Content.CreateProofRequest).keyHandle)
    }

    @Test
    fun `ring vrf key variants sit at wire indices 18 to 23`() {
        val ringScale = RING.toScale()

        assertEquals(
            18,
            variantIndexOf(SsoMessageContent.RegisterRingVrfKeyRequest("game.dot", DerivationIndex32.fromUInt(0u).toScale(), ringScale))
        )
        assertEquals(
            19,
            variantIndexOf(SsoMessageContent.RegisterRingVrfKeyResponse(REQUEST_ID, BSResult.Err(SsoRingVrfErrorScale.Rejected)))
        )
        assertEquals(
            20,
            variantIndexOf(SsoMessageContent.ListRingVrfKeysRequest("game.dot", "peopl.dot", RingVrfKeyDisclosureScale.Anonymized))
        )
        assertEquals(
            21,
            variantIndexOf(SsoMessageContent.ListRingVrfKeysResponse(REQUEST_ID, BSResult.Ok(emptyList())))
        )
        assertEquals(
            22,
            variantIndexOf(SsoMessageContent.RingVrfSignRequest("game.dot", KEY_HANDLE.toScale(), ByteArray(2).toDataByteArray()))
        )
        assertEquals(
            23,
            variantIndexOf(SsoMessageContent.RingVrfSignResponse(REQUEST_ID, BSResult.Err(SsoRingVrfErrorScale.KeyNotRegistered)))
        )
    }

    /** RFC-0024 inserts the key-handle failures before Rejected, shifting Rejected and Unknown. */
    @Test
    fun `ring vrf error variants keep the RFC-0024 order`() {
        assertEquals(2, errorIndexOf(SsoRingVrfErrorScale.KeyNotRegistered))
        assertEquals(3, errorIndexOf(SsoRingVrfErrorScale.KeyNotInRing))
        assertEquals(4, errorIndexOf(SsoRingVrfErrorScale.NotAllowlisted))
        assertEquals(5, errorIndexOf(SsoRingVrfErrorScale.Rejected))
    }

    private fun requestWith(content: SsoSessionRequest.Content) = SsoSessionRequest(
        sessionId = SESSION_ID,
        requestId = REQUEST_ID,
        content = content,
    )

    private fun variantIndexOf(content: SsoMessageContent): Int =
        BinaryScale.encodeToByteArray(SsoSessionMessageV1(content)).first().toInt()

    private fun errorIndexOf(error: SsoRingVrfErrorScale): Int =
        BinaryScale.encodeToByteArray(error).first().toInt()
}
