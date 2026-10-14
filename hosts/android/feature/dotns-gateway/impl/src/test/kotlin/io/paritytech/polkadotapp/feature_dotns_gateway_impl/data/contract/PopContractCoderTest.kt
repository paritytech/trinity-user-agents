@file:OptIn(ExperimentalStdlibApi::class)

package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.contract

import io.novasama.substrate_sdk_android.extensions.fromHex
import org.bouncycastle.jcajce.provider.digest.Keccak
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.TypeReference
import org.web3j.abi.datatypes.Address
import org.web3j.abi.datatypes.Bool
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Function
import org.web3j.abi.datatypes.Type

class PopContractCoderTest {
    @Test
    fun `encodeIsReservedForClaim uses keccak-derived selector`() {
        val encoded = PopContractCoder.encodeIsReservedForClaim("alice")

        assertArrayEquals(selectorOf("isReservedForClaim(string)"), encoded.copyOfRange(0, 4))
    }

    @Test
    fun `encodeChatKey uses keccak-derived selector and node argument`() {
        val node = ByteArray(32) { it.toByte() }
        val encoded = PopContractCoder.encodeChatKey(node)

        assertArrayEquals(selectorOf("chatKey(bytes32)"), encoded.copyOfRange(0, 4))
        assertArrayEquals(node, encoded.copyOfRange(4, 36))
    }

    @Test
    fun `decodeIsReservedForClaim round-trips reserved holder`() {
        val holderHex = "11".repeat(20)
        val output = abiEncodeReturnValues(
            listOf(Bool(true), Address("0x$holderHex")),
            listOf(object : TypeReference<Bool>() {}, object : TypeReference<Address>() {})
        )

        val decoded = PopContractCoder.decodeIsReservedForClaim(output)

        assertNotNull(decoded)
        assertTrue(decoded!!.reserved)
        assertArrayEquals(holderHex.fromHex(), decoded.holder)
    }

    @Test
    fun `decodeIsReservedForClaim round-trips not reserved`() {
        val output = abiEncodeReturnValues(
            listOf(Bool(false), Address("0x" + "00".repeat(20))),
            listOf(object : TypeReference<Bool>() {}, object : TypeReference<Address>() {})
        )

        val decoded = PopContractCoder.decodeIsReservedForClaim(output)

        assertNotNull(decoded)
        assertFalse(decoded!!.reserved)
    }

    @Test
    fun `decodeChatKey round-trips key bytes`() {
        val key = ByteArray(65) { 7 }
        val output = abiEncodeReturnValues(
            listOf(DynamicBytes(key)),
            listOf(object : TypeReference<DynamicBytes>() {})
        )

        assertArrayEquals(key, PopContractCoder.decodeChatKey(output))
    }

    @Test
    fun `decodeChatKey returns null for empty bytes`() {
        val output = abiEncodeReturnValues(
            listOf(DynamicBytes(ByteArray(0))),
            listOf(object : TypeReference<DynamicBytes>() {})
        )

        assertNull(PopContractCoder.decodeChatKey(output))
    }

    @Test
    fun `decodeChatKey returns null for empty output`() {
        assertNull(PopContractCoder.decodeChatKey(ByteArray(0)))
    }

    private fun selectorOf(signature: String): ByteArray {
        return Keccak.Digest256().digest(signature.toByteArray()).copyOfRange(0, 4)
    }

    @Suppress("UNCHECKED_CAST")
    private fun abiEncodeReturnValues(values: List<Type<*>>, typeRefs: List<TypeReference<out Type<*>>>): ByteArray {
        val function = Function("_", values, typeRefs as List<TypeReference<Type<*>>>)
        val encoded = FunctionEncoder.encode(function).fromHex()
        return encoded.copyOfRange(4, encoded.size)
    }
}
