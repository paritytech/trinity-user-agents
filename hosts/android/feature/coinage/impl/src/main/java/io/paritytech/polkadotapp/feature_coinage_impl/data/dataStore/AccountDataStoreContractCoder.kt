package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.FunctionReturnDecoder
import org.web3j.abi.TypeReference
import org.web3j.abi.datatypes.Address
import org.web3j.abi.datatypes.DynamicArray
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Function
import org.web3j.abi.datatypes.Type

// ABI of the AccountDataStore contract (`account-data-store-contract`, `abi/AccountDataStore.json`).
object AccountDataStoreContractCoder {
    @Suppress("UNCHECKED_CAST")
    private val installationsOutput = listOf(
        object : TypeReference<DynamicArray<DynamicBytes>>() {}
    ) as List<TypeReference<Type<*>>>

    fun encodeRegisterInstallation(record: DataByteArray): DataByteArray {
        val function = Function("registerCoinageInstallation", listOf(DynamicBytes(record.value)), emptyList())

        return FunctionEncoder.encode(function).fromHex().toDataByteArray()
    }

    fun encodeGetInstallations(owner: EvmAccountId): DataByteArray {
        val function = Function(
            "getCoinageInstallations",
            listOf(Address(owner.value.toHexString(withPrefix = true))),
            installationsOutput,
        )

        return FunctionEncoder.encode(function).fromHex().toDataByteArray()
    }

    fun decodeGetInstallations(output: DataByteArray): List<DataByteArray> {
        val decoded = FunctionReturnDecoder.decode(output.value.toHexString(withPrefix = true), installationsOutput)

        @Suppress("UNCHECKED_CAST")
        val records = decoded.firstOrNull() as? DynamicArray<DynamicBytes> ?: return emptyList()

        return records.value.map { it.value.toDataByteArray() }
    }
}
