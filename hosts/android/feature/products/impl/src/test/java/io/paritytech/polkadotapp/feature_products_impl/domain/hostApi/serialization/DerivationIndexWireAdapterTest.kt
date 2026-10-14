package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization

import com.google.gson.Gson
import com.google.gson.annotations.JsonAdapter
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import org.junit.Assert.assertEquals
import org.junit.Test

private data class SelectorHolder(
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val derivationIndex: DerivationIndexWire,
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val optionalIndex: DerivationIndexWire? = null,
)

class DerivationIndexWireAdapterTest {
    private val gson = Gson()

    @Test
    fun `plain selector is a json number`() {
        val json = gson.toJson(SelectorHolder(DerivationIndexWire.Plain(5u)))

        assertEquals("""{"derivationIndex":5}""", json)
    }

    @Test
    fun `raw selector is a hex string`() {
        val bytes = ByteArray(DerivationIndex32.SIZE_BYTES) { 0xAB.toByte() }

        val json = gson.toJson(SelectorHolder(DerivationIndexWire.Raw(bytes.toDataByteArray())))

        assertEquals("""{"derivationIndex":"0x${"ab".repeat(DerivationIndex32.SIZE_BYTES)}"}""", json)
    }

    @Test
    fun `number deserializes into a plain selector`() {
        val holder = gson.fromJson("""{"derivationIndex":7}""", SelectorHolder::class.java)

        assertEquals(DerivationIndexWire.Plain(7u), holder.derivationIndex)
        assertEquals(DerivationIndex32.fromUInt(7u), holder.derivationIndex.toDomain().getOrThrow())
    }

    @Test
    fun `hex string deserializes into a raw selector`() {
        val hex = "0x" + "ab".repeat(DerivationIndex32.SIZE_BYTES)

        val holder = gson.fromJson("""{"derivationIndex":"$hex"}""", SelectorHolder::class.java)

        val expected = ByteArray(DerivationIndex32.SIZE_BYTES) { 0xAB.toByte() }.toDataByteArray()
        assertEquals(DerivationIndexWire.Raw(expected), holder.derivationIndex)
    }

    @Test
    fun `absent optional selector stays null`() {
        val holder = gson.fromJson("""{"derivationIndex":1}""", SelectorHolder::class.java)

        assertEquals(null, holder.optionalIndex)
    }

    @Test
    fun `plain index survives a domain round trip as a number`() {
        val domain = DerivationIndex32.fromUInt(42u)

        val json = gson.toJson(SelectorHolder(domain.toWire()))

        assertEquals("""{"derivationIndex":42}""", json)
    }
}
