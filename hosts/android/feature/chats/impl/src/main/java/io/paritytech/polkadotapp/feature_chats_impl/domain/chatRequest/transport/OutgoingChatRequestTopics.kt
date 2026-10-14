package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport

class OutgoingChatRequestTopics(
    val full: ChatRequestTopic.Full,
    val day: ChatRequestTopic.Day,
    val session: ChatRequestTopic.Session
)
