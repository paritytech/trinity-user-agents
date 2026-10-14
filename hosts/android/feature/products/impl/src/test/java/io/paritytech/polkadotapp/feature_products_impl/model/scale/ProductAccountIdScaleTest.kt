package io.paritytech.polkadotapp.feature_products_impl.model.scale

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.common.utils.encodeToByteArrayCatching
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductDerivationIndexScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.toDomain
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductAccountIdScaleTest {
    @Test
    fun `plain selector encodes as variant 0 with a little-endian u32`() {
        val encoded = encode(ProductDerivationIndexScale.Plain(5u))

        assertEquals("0x0005000000", encoded.toHexString(withPrefix = true))
    }

    @Test
    fun `raw selector encodes as variant 1 with 32 unprefixed bytes`() {
        val bytes = ByteArray(DerivationIndex32.SIZE_BYTES) { 0xAB.toByte() }

        val encoded = encode(ProductDerivationIndexScale.Raw(bytes))

        assertEquals("0x01" + "ab".repeat(DerivationIndex32.SIZE_BYTES), encoded.toHexString(withPrefix = true))
    }

    /**
     * Cross-implementation vector: generated from the JS SDK
     * (`ProductAccountId.enc(['browse.dot', derivationIndexOf(5)])`), so a drift in either codec fails here.
     */
    @Test
    fun `product account id matches the js sdk encoding`() {
        val encoded = BinaryScale.encodeToByteArrayCatching(
            ProductAccountId("browse.dot", DerivationIndex32.fromUInt(5u)).toScale()
        ).getOrThrow()

        assertEquals("0x2862726f7773652e646f740005000000", encoded.toHexString(withPrefix = true))
    }

    @Test
    fun `plain selector expands to index_bytes on the domain side`() {
        val decoded = ProductDerivationIndexScale.Plain(7u).toDomain().getOrThrow()

        assertEquals(DerivationIndex32.fromUInt(7u), decoded)
    }

    @Test
    fun `product account id round trips through scale`() {
        val original = ProductAccountId("browse.dot", DerivationIndex32.fromUInt(5u))

        val encoded = BinaryScale.encodeToByteArrayCatching(original.toScale()).getOrThrow()
        val decoded = BinaryScale.decodeFromByteArrayCatching<ProductAccountIdScale>(encoded).getOrThrow()

        assertEquals(original, decoded.toDomain().getOrThrow())
    }

    @Test
    fun `raw product account id round trips through scale`() {
        val raw = DerivationIndex32.fromBytes(ByteArray(DerivationIndex32.SIZE_BYTES) { it.toByte() }.toDataByteArray()).getOrThrow()
        val original = ProductAccountId("browse.dot", raw)

        val encoded = BinaryScale.encodeToByteArrayCatching(original.toScale()).getOrThrow()
        val decoded = BinaryScale.decodeFromByteArrayCatching<ProductAccountIdScale>(encoded).getOrThrow()

        assertEquals(original, decoded.toDomain().getOrThrow())
    }

    private fun encode(value: ProductDerivationIndexScale): ByteArray =
        BinaryScale.encodeToByteArrayCatching(value).getOrThrow()
}
