package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.database.model.ChatRequestLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest

fun ChatRequestLocal.toDomain(): ChatRequest {
    return ChatRequest(
        welcomeMessageId = id,
        timestamp = timestamp,
        direction = direction.toDomain(),
        status = status.toDomain(),
        delivery = toDeliveryDomain(),
    )
}

fun ChatRequest.toLocal(): ChatRequestLocal {
    return ChatRequestLocal(
        id = id,
        timestamp = timestamp,
        direction = direction.toLocal(),
        status = status.toLocal(),
        deliveryStatus = delivery.toLocalStatus(),
        deliveredVia = delivery.toLocalVia(),
        lastDeliveredPeriod = delivery.toLocalPeriod(),
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

private fun ChatRequestLocal.toDeliveryDomain(): ChatRequest.Delivery = when (deliveryStatus) {
    ChatRequestLocal.DeliveryStatus.UNDELIVERED -> ChatRequest.Delivery.Undelivered
    ChatRequestLocal.DeliveryStatus.FAILED -> ChatRequest.Delivery.Failed
    ChatRequestLocal.DeliveryStatus.DELIVERED -> deliveredDomain()
}

// A null via is a row delivered before delivery was tracked: an incoming request or one signed by our username.
private fun ChatRequestLocal.deliveredDomain(): ChatRequest.Delivery = when (deliveredVia) {
    null,
    ChatRequestLocal.DeliveredVia.USERNAME -> ChatRequest.Delivery.Delivered
    ChatRequestLocal.DeliveredVia.NOTIFICATION -> {
        val period = requireNotNull(lastDeliveredPeriod) {
            "Chat request $id delivered via NOTIFICATION must record its period, but lastDeliveredPeriod is null"
        }
        ChatRequest.Delivery.DeliveredAnonymously(period.toUInt())
    }
}

fun ChatRequest.Delivery.toLocalStatus(): ChatRequestLocal.DeliveryStatus = when (this) {
    ChatRequest.Delivery.Undelivered -> ChatRequestLocal.DeliveryStatus.UNDELIVERED
    ChatRequest.Delivery.Failed -> ChatRequestLocal.DeliveryStatus.FAILED
    ChatRequest.Delivery.Delivered,
    is ChatRequest.Delivery.DeliveredAnonymously -> ChatRequestLocal.DeliveryStatus.DELIVERED
}

fun ChatRequest.Delivery.toLocalVia(): ChatRequestLocal.DeliveredVia? = when (this) {
    ChatRequest.Delivery.Delivered -> ChatRequestLocal.DeliveredVia.USERNAME
    is ChatRequest.Delivery.DeliveredAnonymously -> ChatRequestLocal.DeliveredVia.NOTIFICATION
    ChatRequest.Delivery.Undelivered,
    ChatRequest.Delivery.Failed -> null
}

fun ChatRequest.Delivery.toLocalPeriod(): Long? = (this as? ChatRequest.Delivery.DeliveredAnonymously)?.period?.toLong()
