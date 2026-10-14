package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Extrinsic
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.instanceOf
import java.math.BigInteger

private const val SUCCESS_EVENT = "ExtrinsicSuccess"
private const val FAILURE_EVENT = "ExtrinsicFailed"

enum class ExtrinsicOutcome {
    SUCCESS,
    FAILURE,
}

class ExtrinsicWithEvents(
    val extrinsic: Extrinsic.Instance,
    val extrinsicHash: String,
    val events: List<GenericEvent.Instance>,
)

fun ExtrinsicWithEvents.status(): ExtrinsicOutcome? {
    return events.firstNotNullOfOrNull {
        when {
            it.instanceOf(Modules.SYSTEM, SUCCESS_EVENT) -> ExtrinsicOutcome.SUCCESS
            it.instanceOf(Modules.SYSTEM, FAILURE_EVENT) -> ExtrinsicOutcome.FAILURE
            else -> null
        }
    }
}

fun ExtrinsicWithEvents.isSuccess(): Boolean {
    val status = requireNotNull(status()) {
        "Not able to identify extrinsic status"
    }

    return status == ExtrinsicOutcome.SUCCESS
}

fun List<GenericEvent.Instance>.nativeFee(): BigInteger? {
    val event = findEvent(Modules.TRANSACTION_PAYMENT, "TransactionFeePaid") ?: return null
    val (_, actualFee, tip) = event.arguments

    return bindNumber(actualFee) + bindNumber(tip)
}

fun List<GenericEvent.Instance>.requireNativeFee(): BigInteger {
    return requireNotNull(nativeFee()) {
        "No native fee event found"
    }
}

fun List<GenericEvent.Instance>.findEvent(
    module: String,
    event: String,
): GenericEvent.Instance? {
    return find { it.instanceOf(module, event) }
}

fun List<GenericEvent.Instance>.findEventOrThrow(module: String, event: String): GenericEvent.Instance {
    return first { it.instanceOf(module, event) }
}

fun List<GenericEvent.Instance>.findExtrinsicFailure(): GenericEvent.Instance? {
    return findEvent(Modules.SYSTEM, FAILURE_EVENT)
}

fun List<GenericEvent.Instance>.findExtrinsicFailureOrThrow(): GenericEvent.Instance {
    return requireNotNull(findExtrinsicFailure()) {
        "No Extrinsic Failure event found"
    }
}

fun List<GenericEvent.Instance>.findLastEvent(
    module: String,
    event: String,
): GenericEvent.Instance? {
    return findLast { it.instanceOf(module, event) }
}

fun List<GenericEvent.Instance>.hasEvent(
    module: String,
    event: String,
): Boolean {
    return any { it.instanceOf(module, event) }
}

fun List<GenericEvent.Instance>.findAllOfType(
    module: String,
    event: String,
): List<GenericEvent.Instance> {
    return filter { it.instanceOf(module, event) }
}
