package io.paritytech.polkadotapp.feature_dotns_impl.data.contract.abi

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.feature_revive_api.NameHash
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.TypeReference
import org.web3j.abi.datatypes.Address
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Function
import org.web3j.abi.datatypes.Type
import org.web3j.abi.datatypes.Utf8String

@OptIn(ExperimentalStdlibApi::class)
class EvmContractCallerTest {
    @Test
    fun `encodeContenthash produces correct function selector`() {
        val node = NameHash.nameHash("test.dot")
        val encoded = EvmContractCaller.encodeContentHash(node)

        // contenthash(bytes32) selector = keccak256("contenthash(bytes32)")[0:4]
        // = 0xbc1c58d1
        assertEquals("bc1c58d1", encoded.copyOfRange(0, 4).toHexString())
    }

    @Test
    fun `encodeContenthash includes node as bytes32 parameter`() {
        val node = NameHash.nameHash("test.dot")
        val encoded = EvmContractCaller.encodeContentHash(node)

        // After 4-byte selector, next 32 bytes should be the node
        assertArrayEquals(node, encoded.copyOfRange(4, 36))
    }

    @Test
    fun `encodeText produces correct function selector`() {
        val node = NameHash.nameHash("test.dot")
        val encoded = EvmContractCaller.encodeText(node, "manifest")

        // text(bytes32,string) selector = keccak256("text(bytes32,string)")[0:4]
        // = 0x59d1d43c
        assertEquals("59d1d43c", encoded.copyOfRange(0, 4).toHexString())
    }

    @Test
    fun `decodeContentHash round-trips with sample data`() {
        val originalHash = ByteArray(34) { it.toByte() }
        val abiEncoded = abiEncodeBytes(originalHash)

        val decoded = EvmContractCaller.decodeContentHash(abiEncoded)
        assertNotNull(decoded)
        assertArrayEquals(originalHash, decoded)
    }

    @Test
    fun `decodeContentHash returns null for empty output`() {
        val result = EvmContractCaller.decodeContentHash(ByteArray(0))
        assertNull(result)
    }

    @Test
    fun `decodeText round-trips with sample data`() {
        val abiEncoded = abiEncodeString("manifest-value")

        val decoded = EvmContractCaller.decodeText(abiEncoded)
        assertEquals("manifest-value", decoded)
    }

    @Test
    fun `decodeText returns null for empty string`() {
        val abiEncoded = abiEncodeString("")
        val result = EvmContractCaller.decodeText(abiEncoded)
        assertNull(result)
    }

    @Test
    fun `encodeResolver produces correct function selector`() {
        val node = NameHash.nameHash("test.dot")
        val encoded = EvmContractCaller.encodeResolver(node)

        // resolver(bytes32) selector = keccak256("resolver(bytes32)")[0:4] = 0x0178b8bf
        assertEquals("0178b8bf", encoded.copyOfRange(0, 4).toHexString())
    }

    @Test
    fun `decodeAddress round-trips with sample address`() {
        val addressHex = "ab".repeat(20)
        val abiEncoded = abiEncodeAddress("0x$addressHex")

        val decoded = EvmContractCaller.decodeAddress(abiEncoded)
        assertNotNull(decoded)
        assertEquals(addressHex, decoded!!.toHexString())
    }

    @Test
    fun `decodeAddress returns null for zero address`() {
        val abiEncoded = abiEncodeAddress("0x" + "00".repeat(20))
        assertNull(EvmContractCaller.decodeAddress(abiEncoded))
    }

    @Test
    fun `decodeAddress returns null for empty output`() {
        assertNull(EvmContractCaller.decodeAddress(ByteArray(0)))
    }

    @Suppress("UNCHECKED_CAST")
    private fun abiEncodeReturnValue(value: Type<*>, typeRef: TypeReference<out Type<*>>): ByteArray {
        // Encode as a function call with the value as input, then strip the 4-byte selector
        // This is needed because there is no encode alternative to DefaultFunctionReturnDecoder
        val function = Function("_", listOf(value), listOf(typeRef) as List<TypeReference<Type<*>>>)
        val encoded = FunctionEncoder.encode(function).fromHex()
        return encoded.copyOfRange(4, encoded.size) // Strip function selector
    }

    private fun abiEncodeBytes(data: ByteArray): ByteArray {
        return abiEncodeReturnValue(DynamicBytes(data), object : TypeReference<DynamicBytes>() {})
    }

    private fun abiEncodeString(str: String): ByteArray {
        return abiEncodeReturnValue(Utf8String(str), object : TypeReference<Utf8String>() {})
    }

    private fun abiEncodeAddress(hex: String): ByteArray {
        return abiEncodeReturnValue(Address(hex), object : TypeReference<Address>() {})
    }
}
