package io.paritytech.polkadotapp.feature_xcm_api.multiLocation

import io.novasama.substrate_sdk_android.encrypt.json.copyBytes
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.Struct
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindByteArray
import io.paritytech.polkadotapp.chains.network.binding.bindInt
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.chains.util.Geneses
import io.paritytech.polkadotapp.chains.util.structOf
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.padEnd
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.Junction
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.NetworkId
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcm
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.bindVersionedXcm

// ------ Decode ------

fun bindMultiLocation(instance: Any?): RelativeMultiLocation {
    val asStruct = instance.castToStruct()

    return RelativeMultiLocation(
        parents = bindInt(asStruct["parents"]),
        interior = bindInterior((asStruct["interior"]))
    )
}

fun bindVersionedMultiLocation(instance: Any?): VersionedXcm<RelativeMultiLocation> {
    return bindVersionedXcm(instance) { inner, _ -> bindMultiLocation(inner) }
}

private fun bindInterior(instance: Any?): MultiLocation.Interior {
    val asDictEnum = instance.castToDictEnum()

    return when (asDictEnum.name) {
        "Here" -> MultiLocation.Interior.Here

        else -> {
            val junctions = bindJunctions(asDictEnum.value)
            MultiLocation.Interior.Junctions(junctions)
        }
    }
}

private fun bindJunctions(instance: Any?): List<Junction> {
    // Note that Interior.X1 is encoded differently in XCM v3 (a single junction) and V4 (single-element list)
    if (instance is List<*>) {
        return bindList(instance, ::bindJunction)
    } else {
        return listOf(bindJunction(instance))
    }
}

private fun bindJunction(instance: Any?): Junction {
    val asDictEnum = instance.castToDictEnum()

    return when (asDictEnum.name) {
        "GeneralKey" -> Junction.GeneralKey(bindGeneralKey(asDictEnum.value))
        "PalletInstance" -> Junction.PalletInstance(bindNumber(asDictEnum.value))
        "Parachain" -> Junction.ParachainId(bindNumber(asDictEnum.value))
        "GeneralIndex" -> Junction.GeneralIndex(bindNumber(asDictEnum.value))
        "GlobalConsensus" -> bindGlobalConsensusJunction(asDictEnum.value)
        "AccountKey20" -> Junction.AccountKey20(bindAccountIdJunction(asDictEnum.value, AccountId = "key"))
        "AccountId32" -> Junction.AccountId32(bindAccountIdJunction(asDictEnum.value, AccountId = "id"))

        else -> Junction.Unsupported
    }
}

private fun bindGeneralKey(instance: Any?): DataByteArray {
    val keyBytes = if (instance is Struct.Instance) {
        // v3+ structure
        val keyLength = bindInt(instance["length"])
        val keyPadded = bindByteArray(instance["data"])

        keyPadded.copyBytes(0, keyLength)
    } else {
        bindByteArray(instance)
    }

    return keyBytes.toDataByteArray()
}

private fun bindAccountIdJunction(instance: Any?, AccountId: String): AccountId {
    val asStruct = instance.castToStruct()

    return bindAccountId(asStruct[AccountId])
}

private fun bindGlobalConsensusJunction(instance: Any?): Junction {
    val asDictEnum = instance.castToDictEnum()

    return when (asDictEnum.name) {
        "ByGenesis" -> {
            val genesis = bindByteArray(asDictEnum.value).toDataByteArray()
            Junction.GlobalConsensus(networkId = NetworkId.Substrate(genesis))
        }

        "Polkadot" -> Junction.GlobalConsensus(NetworkId.Substrate(Chain.Geneses.POLKADOT))
        "Kusama" -> Junction.GlobalConsensus(NetworkId.Substrate(Chain.Geneses.KUSAMA))
        "Ethereum" -> {
            val chainId = bindInt(asDictEnum.value.castToStruct()["chain_id"])
            Junction.GlobalConsensus(NetworkId.Ethereum(chainId))
        }
        else -> Junction.Unsupported
    }
}

// ------ Encode ------

internal fun RelativeMultiLocation.toEncodableInstanceExt(xcmVersion: XcmVersion) = structOf(
    "parents" to parents.toBigInteger(),
    "interior" to interior.toEncodableInstance(xcmVersion)
)

private fun MultiLocation.Interior.toEncodableInstance(xcmVersion: XcmVersion) = when (this) {
    MultiLocation.Interior.Here -> DictEnum.Entry("Here", null)

    is MultiLocation.Interior.Junctions -> if (junctions.size == 1 && xcmVersion <= XcmVersion.V3) {
        // X1 is encoded as a single junction in V3 and prior
        DictEnum.Entry(
            name = "X1",
            value = junctions.single().toEncodableInstance(xcmVersion)
        )
    } else {
        DictEnum.Entry(
            name = "X${junctions.size}",
            value = junctions.map { it.toEncodableInstance(xcmVersion) }
        )
    }
}

private fun Junction.toEncodableInstance(xcmVersion: XcmVersion) = when (this) {
    is Junction.GeneralKey -> DictEnum.Entry("GeneralKey", encodableGeneralKey(xcmVersion, key))
    is Junction.PalletInstance -> DictEnum.Entry("PalletInstance", index)
    is Junction.ParachainId -> DictEnum.Entry("Parachain", id)
    is Junction.AccountKey20 -> DictEnum.Entry("AccountKey20", accountId.toJunctionAccountIdInstance(AccountId = "key", xcmVersion))
    is Junction.AccountId32 -> DictEnum.Entry("AccountId32", accountId.toJunctionAccountIdInstance(AccountId = "id", xcmVersion))
    is Junction.GeneralIndex -> DictEnum.Entry("GeneralIndex", index)
    is Junction.GlobalConsensus -> toEncodableInstance()
    Junction.Unsupported -> error("Unsupported junction")
}

private fun encodableGeneralKey(xcmVersion: XcmVersion, generalKey: DataByteArray): Any {
    val bytes = generalKey.value

    return if (xcmVersion >= XcmVersion.V3) {
        structOf(
            "length" to bytes.size.toBigInteger(),
            "data" to bytes.padEnd(expectedSize = 32, padding = 0)
        )
    } else {
        bytes
    }
}

private fun Junction.GlobalConsensus.toEncodableInstance(): Any {
    val innerValue = when (networkId) {
        is NetworkId.Ethereum -> networkId.toEncodableInstance()
        is NetworkId.Substrate -> networkId.toEncodableInstance()
    }

    return DictEnum.Entry("GlobalConsensus", innerValue)
}

private fun NetworkId.Ethereum.toEncodableInstance(): Any {
    return DictEnum.Entry("Ethereum", structOf("chain_id" to chainId.toBigInteger()))
}

private fun NetworkId.Substrate.toEncodableInstance(): Any {
    return when (genesisHash) {
        Chain.Geneses.POLKADOT -> DictEnum.Entry("Polkadot", null)
        Chain.Geneses.KUSAMA -> DictEnum.Entry("Kusama", null)
        else -> DictEnum.Entry("ByGenesis", genesisHash)
    }
}

private fun AccountId.toJunctionAccountIdInstance(AccountId: String, xcmVersion: XcmVersion) = structOf(
    "network" to emptyNetworkField(xcmVersion),
    AccountId to value
)

private fun emptyNetworkField(xcmVersion: XcmVersion): Any? {
    return if (xcmVersion >= XcmVersion.V3) {
        // Network in V3+ is encoded as Option<NetworkId>
        null
    } else {
        // Network in V2- is encoded as Enum with Any variant
        DictEnum.Entry("Any", null)
    }
}
