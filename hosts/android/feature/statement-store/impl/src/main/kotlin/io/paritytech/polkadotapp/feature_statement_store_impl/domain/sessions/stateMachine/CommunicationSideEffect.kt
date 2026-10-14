package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

sealed interface CommunicationSideEffect {
    object FetchInitialData : CommunicationSideEffect

    object StartPolling : CommunicationSideEffect
    object StopPolling : CommunicationSideEffect

    class SubmitRequest(val request: StatementTransportEvent.Request) : CommunicationSideEffect
    class SubmitResponse(val response: StatementTransportEvent.Response) : CommunicationSideEffect

    class RequestReceived(val request: StatementTransportEvent.Request) : CommunicationSideEffect
    class ResponseReceived(val respondedMessages: List<EncodedMessage>) : CommunicationSideEffect

    class Compact(val messages: List<EncodedMessage>) : CommunicationSideEffect
    class MessagesCompacted(val commit: EncodedMessage, val originals: List<EncodedMessage>) : CommunicationSideEffect

    class NotifyMessageTooLarge(val message: EncodedMessage, val maxAllowedSize: InformationSize) : CommunicationSideEffect
}
