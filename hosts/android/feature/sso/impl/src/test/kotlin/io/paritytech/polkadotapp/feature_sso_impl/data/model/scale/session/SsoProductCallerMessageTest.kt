package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertEquals
import org.junit.Test

private val SESSION_ID = SsoSessionId("session")
private val TLD = requireNotNull(DotNsTld.parse("dot"))

private val GAME = ProductId.fromStoredValue("game.dot").toCallerScale()

private val RING = RingLocation(
    chainId = ByteArray(32) { 7 }.toDataByteArray(),
    junctions = listOf(
        RingLocationJunction.PalletInstance(9u),
        RingLocationJunction.CollectionId(RingCollectionId.paddedString("pop:polkadot.network/people").value),
    )
)

/**
 * Every product-originated request opens with its caller: the product id, then the executable
 * kind. The fixtures are the bytes the TrUAPI core's SSO wire tests pin.
 */
class SsoProductCallerMessageTest {
    @Test
    fun `a resource allocation request names its caller`() {
        val request = (
            "286d2d7265736f757263650005547472756170692d706c617967726f756e642e646f74" +
                "001000010200090000000301"
            ).fromHex().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()

        val content = request.content as SsoSessionRequest.Content.ResourceAllocationRequest
        assertEquals("truapi-playground.dot", content.callingProduct.value)
        assertEquals(4, content.resources.size)
    }

    @Test
    fun `a transaction request names its caller before its payload`() {
        val request = (
            "306d2d70726f647563742d74780007547472756170692d706c617967726f756e642e646f74" +
                "0000547472756170692d706c617967726f756e642e646f740000000000202122232425" +
                "262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b" +
                "4e6f6e6365040108020300"
            ).fromHex().toSsoSessionRequest(SESSION_ID, TLD).getOrThrow()

        val content = request.content as SsoSessionRequest.Content.CreateTransactionRequest
        assertEquals("truapi-playground.dot", content.callingProduct.value)
        assertEquals("truapi-playground.dot", content.request.payload.signer.productId)
    }

    @Test
    fun `ring vrf key requests encode the caller as the pairing host does`() {
        val handle = ProductAccountId("peopl.dot", DerivationIndex32.fromUInt(0u))
        val messages = listOf(
            SsoMessageContent.RegisterRingVrfKeyRequest(GAME, DerivationIndex32.fromUInt(4u).toScale(), RING.toScale()),
            SsoMessageContent.ListRingVrfKeysRequest(GAME, "peopl.dot", RingVrfKeyDisclosureScale.PublicKey),
            SsoMessageContent.RingVrfSignRequest(GAME, handle.toScale(), ByteArray(16) { it.toByte() }.toDataByteArray()),
        )
        val expected = listOf(
            "122067616d652e646f740000040000000707070707070707070707070707070707070707070707070707070707070707" +
                "0800090180706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652020202020",
            "142067616d652e646f74002470656f706c2e646f7401",
            "162067616d652e646f74002470656f706c2e646f74000000000040000102030405060708090a0b0c0d0e0f",
        )

        messages.zip(expected).forEach { (message, hex) ->
            assertEquals(hex, BinaryScale.encodeToByteArray(SsoSessionMessageV1(message)).toHexString(withPrefix = false))
        }
    }
}
