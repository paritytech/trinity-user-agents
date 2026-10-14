package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine

import io.paritytech.polkadotapp.feature_statement_store_api.domain.CompactedBatch
import io.paritytech.polkadotapp.feature_statement_store_api.domain.IsResponded
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

sealed interface CommunicationStateEvent {
    class InitialDataFetched(
        val outgoingPendingRequest: StatementTransportEvent.Request?,
        val incomingRequests: Map<StatementTransportEvent.Request, IsResponded>,
        val lastUsedExpiry: ULong
    ) : CommunicationStateEvent

    class SubmitMessage(val message: EncodedMessage) : CommunicationStateEvent

    class CompactionCompleted(val batches: List<CompactedBatch>) : CommunicationStateEvent
    object CompactionFailed : CommunicationStateEvent

    class RequestSent(val request: StatementTransportEvent.Request) : CommunicationStateEvent
    class ResponseSent(val response: StatementTransportEvent.Response) : CommunicationStateEvent

    class RequestReceived(val request: StatementTransportEvent.Request) : CommunicationStateEvent
    class ResponseReceived(val response: StatementTransportEvent.Response) : CommunicationStateEvent

    class InvalidateSession(val error: Throwable) : CommunicationStateEvent
}
