package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ChatRequest(
    val welcomeMessageId: ChatMessageId,
    val timestamp: Long,
    val direction: Direction,
    val status: Status
) {
    val id: ChatRequestId = welcomeMessageId

    enum class Direction {
        INCOMING,
        OUTGOING
    }

    enum class Status {
        PENDING,
        ACCEPTED,
        DECLINED
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
