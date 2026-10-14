package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.contract

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.FunctionReturnDecoder
import org.web3j.abi.TypeReference
import org.web3j.abi.datatypes.Address
import org.web3j.abi.datatypes.Bool
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Function
import org.web3j.abi.datatypes.Type
import org.web3j.abi.datatypes.Utf8String
import org.web3j.abi.datatypes.generated.Bytes32

object PopContractCoder {
    class ReservationForClaim(val reserved: Boolean, val holder: ByteArray)

    @Suppress("UNCHECKED_CAST")
    private val isReservedForClaimOutputParams = listOf(
        object : TypeReference<Bool>() {},
        object : TypeReference<Address>() {}
    ) as List<TypeReference<Type<*>>>

    @Suppress("UNCHECKED_CAST")
    private val chatKeyOutputParams =
        listOf(object : TypeReference<DynamicBytes>() {}) as List<TypeReference<Type<*>>>

    fun encodeIsReservedForClaim(baseLabel: String): ByteArray {
        val function = Function("isReservedForClaim", listOf(Utf8String(baseLabel)), isReservedForClaimOutputParams)
        return FunctionEncoder.encode(function).fromHex()
    }

    fun decodeIsReservedForClaim(output: ByteArray): ReservationForClaim? {
        val decoded = FunctionReturnDecoder.decode(output.toHexString(withPrefix = true), isReservedForClaimOutputParams)
        if (decoded.size < 2) return null
        val reserved = (decoded[0] as Bool).value
        val holder = (decoded[1] as Address).value.fromHex()
        return ReservationForClaim(reserved, holder)
    }

    fun encodeChatKey(node: ByteArray): ByteArray {
        val function = Function("chatKey", listOf(Bytes32(node)), chatKeyOutputParams)
        return FunctionEncoder.encode(function).fromHex()
    }

    fun decodeChatKey(output: ByteArray): ByteArray? {
        val decoded = FunctionReturnDecoder.decode(output.toHexString(withPrefix = true), chatKeyOutputParams)
        if (decoded.isEmpty()) return null
        val bytes = (decoded[0] as DynamicBytes).value
        return if (bytes.isEmpty()) null else bytes
    }
}
