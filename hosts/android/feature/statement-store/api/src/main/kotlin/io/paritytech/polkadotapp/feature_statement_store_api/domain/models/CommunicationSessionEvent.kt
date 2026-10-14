package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

import io.paritytech.polkadotapp.common.utils.InformationSize

sealed interface CommunicationSessionEvent {
    class NewMessagesReceived(val requestId: String, val messages: List<EncodedMessage>) : CommunicationSessionEvent
    class MessageIsTooLarge(val message: EncodedMessage, val maxAllowedSize: InformationSize) : CommunicationSessionEvent
    class ResponseReceived(val respondedMessages: List<EncodedMessage>) : CommunicationSessionEvent
    class MessagesSentSuccessfully(val messages: List<EncodedMessage>) : CommunicationSessionEvent
    class MessagesFailedToSend(val messages: List<EncodedMessage>, val error: Throwable) : CommunicationSessionEvent
    class MessagesCompacted(val compacted: EncodedMessage, val originals: List<EncodedMessage>) : CommunicationSessionEvent
    class SessionFailed(val error: Throwable) : CommunicationSessionEvent
    object SentMessagesNotFound : CommunicationSessionEvent
}
