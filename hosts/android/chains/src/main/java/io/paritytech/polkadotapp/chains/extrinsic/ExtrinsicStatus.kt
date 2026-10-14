package io.paritytech.polkadotapp.chains.extrinsic

import io.novasama.substrate_sdk_android.wsrpc.subscription.response.SubscriptionChange

sealed class ExtrinsicStatus(val terminal: Boolean) {
    sealed interface Failure

    sealed interface Submitted {
        val extrinsicHash: String
    }

    data class FailedToSubmit(val exception: Throwable) : ExtrinsicStatus(terminal = true), Failure

    data class Ready(override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted

    data class Broadcast(override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted

    data class InBlock(val blockHash: String, override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted

    data class Finalized(val blockHash: String, override val extrinsicHash: String) : ExtrinsicStatus(terminal = true), Submitted

    data class Invalid(override val extrinsicHash: String) : ExtrinsicStatus(terminal = true), Submitted, Failure

    /** Accepted into the pool but not yet valid — its nonce is ahead of the account's. */
    data class Future(override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted

    /** The block that carried the extrinsic left the canonical chain; it may still be re-included. */
    data class Retracted(val blockHash: String, override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted

    /** Evicted from the pool. The bytes stay valid, so resubmitting the same extrinsic can still work. */
    data class Dropped(override val extrinsicHash: String) : ExtrinsicStatus(terminal = true), Submitted, Failure

    /** Another extrinsic took this one's (sender, nonce). Resubmitting the same bytes cannot succeed. */
    data class Usurped(val by: String, override val extrinsicHash: String) : ExtrinsicStatus(terminal = true), Submitted, Failure

    // rawStatus preserves any node status this class does not model, for diagnostics.
    data class Other(val rawStatus: String, override val extrinsicHash: String) : ExtrinsicStatus(terminal = false), Submitted
}

private const val STATUS_READY = "ready"
private const val STATUS_FUTURE = "future"
private const val STATUS_BROADCAST = "broadcast"
private const val STATUS_IN_BLOCK = "inBlock"
private const val STATUS_RETRACTED = "retracted"
private const val STATUS_FINALIZED = "finalized"
private const val STATUS_INVALID = "invalid"
private const val STATUS_DROPPED = "dropped"
private const val STATUS_USURPED = "usurped"
private const val STATUS_FINALITY_TIMEOUT = "finalityTimeout"

fun SubscriptionChange.asExtrinsicStatus(extrinsicHash: String): ExtrinsicStatus {
    return when (val result = params.result) {
        STATUS_READY -> ExtrinsicStatus.Ready(extrinsicHash)
        STATUS_FUTURE -> ExtrinsicStatus.Future(extrinsicHash)
        STATUS_INVALID -> ExtrinsicStatus.Invalid(extrinsicHash)
        STATUS_DROPPED -> ExtrinsicStatus.Dropped(extrinsicHash)
        is Map<*, *> ->
            when {
                STATUS_BROADCAST in result -> ExtrinsicStatus.Broadcast(extrinsicHash)
                STATUS_IN_BLOCK in result -> ExtrinsicStatus.InBlock(extractHash(result, STATUS_IN_BLOCK), extrinsicHash)
                STATUS_RETRACTED in result -> ExtrinsicStatus.Retracted(extractHash(result, STATUS_RETRACTED), extrinsicHash)
                STATUS_FINALIZED in result -> ExtrinsicStatus.Finalized(extractHash(result, STATUS_FINALIZED), extrinsicHash)
                STATUS_FINALITY_TIMEOUT in result -> ExtrinsicStatus.Finalized(extractHash(result, STATUS_FINALITY_TIMEOUT), extrinsicHash)
                STATUS_USURPED in result -> ExtrinsicStatus.Usurped(extractHash(result, STATUS_USURPED), extrinsicHash)
                else -> ExtrinsicStatus.Other(rawStatus = result.toString(), extrinsicHash = extrinsicHash)
            }
        else -> ExtrinsicStatus.Other(rawStatus = result.toString(), extrinsicHash = extrinsicHash)
    }
}

private fun extractHash(
    map: Map<*, *>,
    key: String,
): String {
    return map[key] as? String ?: unknownStructure()
}

private fun unknownStructure(): Nothing = throw IllegalArgumentException("Unknown extrinsic status structure")
