package io.paritytech.polkadotapp.chains.multiNetwork.chain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHexOrNull
import io.novasama.substrate_sdk_android.runtime.definitions.types.toHexUntyped
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.utils.HexString

typealias UntypedOrmlCurrencyId = AsRawScaleValue

fun Type.Orml.currencyId(runtime: RuntimeSnapshot): UntypedOrmlCurrencyId {
    val currencyIdType = runtime.typeRegistry[currencyIdType]
        ?: error("Cannot find type $currencyIdType")

    return currencyIdType.fromHex(runtime, currencyIdScale)
        .let(::UntypedOrmlCurrencyId)
}

fun Type.Orml.currencyIdHex(runtime: RuntimeSnapshot, currencyId: UntypedOrmlCurrencyId): HexString {
    val currencyIdType = runtime.typeRegistry[currencyIdType]
        ?: error("Cannot find type $currencyIdType")

    return currencyIdType.toHexUntyped(runtime, currencyId)
}

fun Type.Orml.currencyIdOrNull(runtime: RuntimeSnapshot): UntypedOrmlCurrencyId? {
    val currencyIdType = runtime.typeRegistry[currencyIdType] ?: return null

    return currencyIdType.fromHexOrNull(runtime, currencyIdScale)
        ?.let(::UntypedOrmlCurrencyId)
}

context(withRuntime: WithRuntime)
fun Type.Orml.currencyId(): UntypedOrmlCurrencyId {
    return currencyId(withRuntime.runtime)
}
