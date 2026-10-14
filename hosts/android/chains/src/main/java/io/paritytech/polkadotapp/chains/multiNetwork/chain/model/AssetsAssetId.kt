package io.paritytech.polkadotapp.chains.multiNetwork.chain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.RuntimeType
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.toHexUntyped
import io.novasama.substrate_sdk_android.runtime.metadata.callOrNull
import io.novasama.substrate_sdk_android.runtime.metadata.moduleOrNull
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import java.math.BigInteger

sealed interface AssetsAssetId {
    @JvmInline
    value class ScaleEncoded(val scaleHex: String) : AssetsAssetId

    @JvmInline
    value class Number(val value: BigInteger) : AssetsAssetId
}

fun AssetsAssetId.asNumberOrNull(): BigInteger? {
    return (this as? AssetsAssetId.Number)?.value
}

fun AssetsAssetId.asNumberOrThrow(): BigInteger {
    return (this as AssetsAssetId.Number).value
}

fun AssetsAssetId.asScaleEncodedOrThrow(): String {
    return (this as AssetsAssetId.ScaleEncoded).scaleHex
}

fun AssetsAssetId.asScaleEncodedOrNull(): String? {
    return (this as? AssetsAssetId.ScaleEncoded)?.scaleHex
}

fun AssetsAssetId.isScaleEncoded(): Boolean {
    return this is AssetsAssetId.ScaleEncoded
}

typealias UntypedAssetsAssetId = AsRawScaleValue

fun Chain.Asset.Type.Assets.prepareIdForEncoding(runtimeSnapshot: RuntimeSnapshot): UntypedAssetsAssetId {
    val raw = when (val id = id) {
        is AssetsAssetId.Number -> id.value

        is AssetsAssetId.ScaleEncoded -> {
            val assetIdType = assetIdScaleType(runtimeSnapshot, palletNameOrDefault())

            assetIdType!!.fromHex(runtimeSnapshot, id.scaleHex)!!
        }
    }

    return AsRawScaleValue(raw)
}

fun Chain.Asset.Type.Assets.hasSameId(runtimeSnapshot: RuntimeSnapshot, dynamicInstanceId: AsRawScaleValue): Boolean {
    return runCatching {
        when (val id = id) {
            is AssetsAssetId.Number -> id.value == bindNumber(dynamicInstanceId)

            is AssetsAssetId.ScaleEncoded -> {
                val assetIdType = assetIdScaleType(runtimeSnapshot, palletNameOrDefault())
                val otherScale = assetIdType!!.toHexUntyped(runtimeSnapshot, dynamicInstanceId.value)

                id.scaleHex == otherScale
            }
        }
    }
        .logFailure("Failed to compare asset ids")
        .getOrDefault(false)
}

context(withRuntime: WithRuntime)
fun Chain.Asset.Type.Assets.prepareIdForEncoding(): UntypedAssetsAssetId {
    return prepareIdForEncoding(withRuntime.runtime)
}

fun Chain.Asset.Type.Assets.palletNameOrDefault(): String {
    return palletName ?: Modules.ASSETS
}

private fun assetIdScaleType(
    runtimeSnapshot: RuntimeSnapshot,
    palletName: String
): RuntimeType<*, *>? {
    val transferCall = runtimeSnapshot.metadata.moduleOrNull(palletName)?.callOrNull("transfer")
    return transferCall?.arguments?.firstOrNull()?.type
}
