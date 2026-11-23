package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ChatRequest(
    val welcomeMessageId: ChatMessageId,
    val timestamp: Long,
    val direction: Direction,
    val status: Status,
    val delivery: Delivery,
) {
    val id: ChatRequestId = welcomeMessageId

    enum class Direction {
        INCOMING,
        OUTGOING
    }

    enum class Status {
        PENDING,
        ACCEPTED,
        DECLINED,
        CANCELLED
    }

    sealed interface Delivery {
        /** Recorded locally, not yet on the statement store. */
        data object Undelivered : Delivery

        /** Can never be delivered, e.g. too large for any statement account. */
        data object Failed : Delivery

        /** On the statement store and never renewed by us: an incoming request, or ours from before delivery was tracked. */
        data object DeliveredUntracked : Delivery

        /** On the statement store, signed by the per-[period] notification account; needs renewal each period. */
        data class DeliveredAnonymously(val period: UInt) : Delivery

        /**
         * On the statement store, signed in [period] by our own account because no notification slot was free and the
         * user allowed it. Still renewed each period, so it goes back to a notification account once a slot frees up.
         */
        data class DeliveredByUsername(val period: UInt) : Delivery
    }
}

fun ChatRequest.isOutgoing(): Boolean {
    return direction == ChatRequest.Direction.OUTGOING
}

fun ChatRequest.isIncoming(): Boolean {
    return direction == ChatRequest.Direction.INCOMING
}

fun ChatRequest.userDeclinedIncomingRequest(): Boolean {
    return isIncoming() && status == ChatRequest.Status.DECLINED
}

fun ChatRequest.isPendingIncoming(): Boolean {
    return isIncoming() && status == ChatRequest.Status.PENDING
}

fun ChatRequest.isPendingOutgoing(): Boolean {
    return isOutgoing() && status == ChatRequest.Status.PENDING
}
