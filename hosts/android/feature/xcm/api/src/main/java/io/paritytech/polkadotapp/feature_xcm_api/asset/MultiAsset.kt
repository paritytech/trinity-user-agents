package io.paritytech.polkadotapp.feature_xcm_api.asset

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.util.structOf
import io.paritytech.polkadotapp.common.data.substrate.cast
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.common.data.substrate.incompatible
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcm
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.bindVersionedXcm

data class MultiAsset private constructor(
    val id: MultiAssetId,
    val fungibility: Fungibility,
) : VersionedToDynamicScaleInstance {
    companion object {
        fun bind(decodedInstance: Any?, xcmVersion: XcmVersion): MultiAsset {
            val asStruct = decodedInstance.castToStruct()
            return MultiAsset(
                id = bindMultiAssetId(asStruct["id"], xcmVersion),
                fungibility = Fungibility.bind(asStruct["fun"])
            )
        }

        fun from(
            multiLocation: RelativeMultiLocation,
            amount: Balance
        ): MultiAsset {
            // Substrate doesn't allow zero balance starting from xcm v3
            val positiveAmount = amount.coerceAtLeast(Balance.ONE)

            return MultiAsset(
                id = MultiAssetId(multiLocation),
                fungibility = Fungibility.Fungible(positiveAmount)
            )
        }
    }

    sealed class Fungibility : ToDynamicScaleInstance {
        companion object {
            fun bind(decodedInstance: Any?): Fungibility {
                val asEnum = decodedInstance.castToDictEnum()

                return when (asEnum.name) {
                    "Fungible" -> Fungible(bindBalance(asEnum.value))
                    else -> incompatible()
                }
            }
        }

        data class Fungible(val amount: Balance) : Fungibility() {
            override fun toEncodableInstance(): Any {
                return DictEnum.Entry(name = "Fungible", value = amount.value)
            }
        }
    }

    override fun toEncodableInstance(xcmVersion: XcmVersion): Any {
        return structOf(
            "fun" to fungibility.toEncodableInstance(),
            "id" to id.toEncodableInstance(xcmVersion)
        )
    }
}

fun MultiAsset.requireFungible(): MultiAsset.Fungibility.Fungible {
    return fungibility.cast()
}

@JvmInline
value class MultiAssets(val value: List<MultiAsset>) : VersionedToDynamicScaleInstance {
    companion object {
        fun bind(decodedInstance: Any?, xcmVersion: XcmVersion): MultiAssets {
            val assets = bindList(decodedInstance) { MultiAsset.bind(it, xcmVersion) }
            return MultiAssets(assets)
        }

        fun bindVersioned(decodedInstance: Any?): VersionedMultiAssets {
            return bindVersionedXcm(decodedInstance, MultiAssets::bind)
        }
    }

    constructor(vararg assets: MultiAsset) : this(assets.toList())

    override fun toEncodableInstance(xcmVersion: XcmVersion): Any {
        return value.map { it.toEncodableInstance(xcmVersion) }
    }

    override fun toString(): String {
        return value.toString()
    }
}

fun List<MultiAsset>.intoMultiAssets(): MultiAssets {
    return MultiAssets(this)
}

fun MultiAsset.intoMultiAssets(): MultiAssets {
    return MultiAssets(listOf(this))
}

typealias VersionedMultiAsset = VersionedXcm<MultiAsset>
typealias VersionedMultiAssets = VersionedXcm<MultiAssets>
