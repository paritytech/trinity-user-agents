package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.Struct
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.common.data.substrate.cast
import io.paritytech.polkadotapp.common.data.substrate.getTyped
import io.paritytech.polkadotapp.common.data.substrate.incompatible
import io.paritytech.polkadotapp.common.data.substrate.requireType
import java.math.BigInteger

class EventRecord(val phase: Phase, val event: GenericEvent.Instance)

sealed class Phase {
    class ApplyExtrinsic(val extrinsicId: BigInteger) : Phase()

    data object Finalization : Phase()

    data object Initialization : Phase()
}

fun bindEventRecords(decoded: Any?): List<EventRecord> = bindList(decoded, ::bindEventRecord)

fun bindEvent(decoded: Any?): GenericEvent.Instance {
    return decoded.cast()
}

fun bindEventRecord(dynamicInstance: Any?): EventRecord {
    requireType<Struct.Instance>(dynamicInstance)

    val phaseDynamic = dynamicInstance.getTyped<DictEnum.Entry<*>>("phase")

    val phase =
        when (phaseDynamic.name) {
            "ApplyExtrinsic" -> Phase.ApplyExtrinsic(phaseDynamic.value.cast())
            "Finalization" -> Phase.Finalization
            "Initialization" -> Phase.Initialization
            else -> incompatible()
        }

    val dynamicEvent = dynamicInstance.getTyped<GenericEvent.Instance>("event")

    return EventRecord(phase, dynamicEvent)
}
