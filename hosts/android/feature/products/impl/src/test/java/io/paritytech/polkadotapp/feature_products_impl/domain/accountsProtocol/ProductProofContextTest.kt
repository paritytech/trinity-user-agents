package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ProductProofContextTest {
    @Test
    fun `context bytes match the RFC-0004 test vector`() {
        val context = ProductProofContext(
            productId = ProductId.fromStoredValue("voting.dot"),
            suffix = DerivationIndex32.fromUInt(0u),
        )

        // blake2b256(utf8("product/voting.dot/") ++ index_bytes(0)) — generated once and pinned.
        val expected = DataByteArray.fromHex("0xfc8e5a62a2abf020f4f5bc5d00c06c18404674804c8dacd5198357c5c761440d")

        assertArrayEquals(expected.value, context.productContextBytes().value)
    }

    @Test
    fun `different products yield different contexts for the same suffix`() {
        val suffix = DerivationIndex32.fromUInt(0u)
        val a = ProductProofContext(ProductId.fromStoredValue("a.dot"), suffix).productContextBytes()
        val b = ProductProofContext(ProductId.fromStoredValue("b.dot"), suffix).productContextBytes()

        assertFalse(a.value.contentEquals(b.value))
    }

    @Test
    fun `different suffixes yield different contexts for the same product`() {
        val productId = ProductId.fromStoredValue("a.dot")
        val first = ProductProofContext(productId, DerivationIndex32.fromUInt(0u)).productContextBytes()
        val second = ProductProofContext(productId, DerivationIndex32.fromUInt(1u)).productContextBytes()

        assertFalse(first.value.contentEquals(second.value))
    }

    @Test
    fun `plain and raw selectors share one context space`() {
        val productId = ProductId.fromStoredValue("a.dot")
        val plain = DerivationIndex32.fromUInt(3u)
        val raw = DerivationIndex32.fromBytes(plain.bytes.value.toDataByteArray()).getOrThrow()

        val fromPlain = ProductProofContext(productId, plain).productContextBytes()
        val fromRaw = ProductProofContext(productId, raw).productContextBytes()

        assertArrayEquals(fromPlain.value, fromRaw.value)
    }
}
