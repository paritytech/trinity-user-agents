package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.database.model.ChatRequestLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest

fun ChatRequestLocal.toDomain(): ChatRequest {
    return ChatRequest(
        welcomeMessageId = id,
        timestamp = timestamp,
        direction = direction.toDomain(),
        status = status.toDomain()
    )
}

fun ChatRequest.toLocal(): ChatRequestLocal {
    return ChatRequestLocal(
        id = id,
        timestamp = timestamp,
        direction = direction.toLocal(),
        status = status.toLocal()
    )
}

private fun ChatRequestLocal.Direction.toDomain(): ChatRequest.Direction {
    return when (this) {
        ChatRequestLocal.Direction.INCOMING -> ChatRequest.Direction.INCOMING
        ChatRequestLocal.Direction.OUTGOING -> ChatRequest.Direction.OUTGOING
    }
}

private fun ChatRequest.Direction.toLocal(): ChatRequestLocal.Direction {
    return when (this) {
        ChatRequest.Direction.INCOMING -> ChatRequestLocal.Direction.INCOMING
        ChatRequest.Direction.OUTGOING -> ChatRequestLocal.Direction.OUTGOING
    }
}

private fun ChatRequestLocal.Status.toDomain(): ChatRequest.Status {
    return when (this) {
        ChatRequestLocal.Status.PENDING -> ChatRequest.Status.PENDING
        ChatRequestLocal.Status.ACCEPTED -> ChatRequest.Status.ACCEPTED
        ChatRequestLocal.Status.DECLINED -> ChatRequest.Status.DECLINED
    }
}

private fun ChatRequest.Status.toLocal(): ChatRequestLocal.Status {
    return when (this) {
        ChatRequest.Status.PENDING -> ChatRequestLocal.Status.PENDING
        ChatRequest.Status.ACCEPTED -> ChatRequestLocal.Status.ACCEPTED
        ChatRequest.Status.DECLINED -> ChatRequestLocal.Status.DECLINED
    }
}
