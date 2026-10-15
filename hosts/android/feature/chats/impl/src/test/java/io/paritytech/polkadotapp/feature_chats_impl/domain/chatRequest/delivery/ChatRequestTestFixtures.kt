package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest

fun outgoingRequest(id: String, delivery: ChatRequest.Delivery): ChatRequest = ChatRequest(
    welcomeMessageId = id,
    timestamp = 1_000L,
    direction = ChatRequest.Direction.OUTGOING,
    status = ChatRequest.Status.PENDING,
    delivery = delivery,
)

fun deliveryAccount(seed: Byte, period: UInt): ChatRequestDeliveryAccount {
    val keypair: Sr25519Keypair = mockk { every { publicKey } returns ByteArray(32) { seed } }
    return ChatRequestDeliveryAccount(keypair, period)
}
