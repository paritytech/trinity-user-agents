package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.stateMachine.StateMachine
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.IsResponded
import io.paritytech.polkadotapp.feature_statement_store_api.domain.MessageCompactor
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionEvent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionIdParams
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.createSessionIdParams
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.defaultExpiry
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.getRequest
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.getRequests
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.hasRequest
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationSideEffect
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationStateEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states.Initialization
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

class RealCommunicationSession(
    override val localAccount: SessionAccount,
    override val remoteAccount: SessionAccount,
    private val communicationTransport: CommunicationTransport,
    private val encryption: CommunicationEncryption,
    private val maxStatementSize: InformationSize,
    private val compactor: MessageCompactor?,
    scope: CoroutineScope
) : CommunicationSession, CoroutineScope by scope {
    private val outgoingSessionIdParams: SessionIdParams = createSessionIdParams(localAccount, remoteAccount)
    private val incomingSessionIdParams: SessionIdParams = createSessionIdParams(remoteAccount, localAccount)

    override val incomingSessionId = deriveCommunicationTopic(sender = remoteAccount, receiver = localAccount, encryption)

    private val stateMachine = StateMachine(
        initialState = Initialization(emptyList(), maxStatementSize, canCompact = compactor != null),
        coroutineScope = this
    )

    private var incomingEventsPollingJob: Job? = null

    private val eventsFlow = MutableSharedFlow<CommunicationSessionEvent>()

    init {
        listenSideEffects()
    }

    private fun listenSideEffects() = launch {
        for (effect in stateMachine.sideEffects) {
            when (effect) {
                is CommunicationSideEffect.FetchInitialData -> {
                    fetchInitialData()
                }

                is CommunicationSideEffect.SubmitRequest -> {
                    submitRequest(effect.request)
                }

                is CommunicationSideEffect.SubmitResponse -> {
                    submitResponse(effect.response)
                }

                is CommunicationSideEffect.StartPolling -> {
                    startPollingIncomingEvents()
                }

                is CommunicationSideEffect.StopPolling -> {
                    stopPollingIncomingEvents()
                }

                is CommunicationSideEffect.RequestReceived -> {
                    handleIncomingRequest(effect.request)
                }

                is CommunicationSideEffect.ResponseReceived -> {
                    handleIncomingResponse(effect.respondedMessages)
                }

                is CommunicationSideEffect.Compact -> {
                    compactMessages(effect.messages)
                }

                is CommunicationSideEffect.MessagesCompacted -> {
                    handleMessagesCompacted(effect.commit, effect.originals)
                }

                is CommunicationSideEffect.NotifyMessageTooLarge -> {
                    handleTooLargeMessage(effect.message, effect.maxAllowedSize)
                }
            }
        }
    }

    private fun fetchInitialData() = launch {
        val outgoingEventsDeferred = async { communicationTransport.fetchOutgoing() }
        val incomingEventsDeferred = async { communicationTransport.fetchIncoming() }

        val outgoingEventsResult = outgoingEventsDeferred.await()
        val incomingEventsResult = incomingEventsDeferred.await()

        val combinedResult = outgoingEventsResult.flatMap { outgoingEvents ->
            incomingEventsResult.map { incomingEvents ->
                outgoingEvents to incomingEvents
            }
        }

        combinedResult
            .onSuccess { (rawOutgoingEvents, rawIncomingEvents) ->
                val outgoingEvents = rawOutgoingEvents.sortedByDescending { it.expiry }
                val incomingEvents = rawIncomingEvents.sortedByDescending { it.expiry }

                if (outgoingEvents.hasRequest().not()) {
                    eventsFlow.emit(CommunicationSessionEvent.SentMessagesNotFound)
                }

                val lastUsedExpiry = outgoingEvents.defaultExpiry()
                val outgoingPendingRequest = restoreOutgoingPendingRequest(outgoingEvents, incomingEvents)
                val incomingRequests = restoreIncomingRequests(outgoingEvents, incomingEvents)

                stateMachine.onEvent(
                    CommunicationStateEvent.InitialDataFetched(
                        outgoingPendingRequest = outgoingPendingRequest,
                        incomingRequests = incomingRequests,
                        lastUsedExpiry = lastUsedExpiry
                    )
                )
            }
            .onFailure {
                eventsFlow.emit(CommunicationSessionEvent.SessionFailed(it))
            }
    }

    private fun restoreOutgoingPendingRequest(
        outgoingEvents: List<StatementTransportEvent>,
        incomingEvents: List<StatementTransportEvent>
    ): StatementTransportEvent.Request? {
        var outgoingPendingRequest: StatementTransportEvent.Request? = null

        val lastOutgoingRequest = outgoingEvents.getRequest()

        if (lastOutgoingRequest != null) {
            val incomingResponse = incomingEvents
                .find { it.requestId == lastOutgoingRequest.requestId && it is StatementTransportEvent.Response }

            if (incomingResponse == null) {
                outgoingPendingRequest = lastOutgoingRequest
            }
        }

        return outgoingPendingRequest
    }

    private fun restoreIncomingRequests(
        outgoingEvents: List<StatementTransportEvent>,
        incomingEvents: List<StatementTransportEvent>
    ): Map<StatementTransportEvent.Request, IsResponded> {
        val respondedRequestIds = outgoingEvents
            .filterIsInstance<StatementTransportEvent.Response>()
            .mapToSet { it.requestId }

        return incomingEvents.getRequests().associateWith { it.requestId in respondedRequestIds }
    }

    private fun submitRequest(request: StatementTransportEvent.Request) {
        launch {
            communicationTransport
                .submitRequest(request)
                .onSuccess {
                    stateMachine.onEvent(CommunicationStateEvent.RequestSent(request))

                    eventsFlow.emit(CommunicationSessionEvent.MessagesSentSuccessfully(request.messages))
                }
                .logFailure("Failed to submit request")
                .onFailure {
                    stateMachine.onEvent(CommunicationStateEvent.InvalidateSession(it))
                    eventsFlow.emit(CommunicationSessionEvent.MessagesFailedToSend(request.messages, it))
                }
        }
    }

    private fun submitResponse(response: StatementTransportEvent.Response) {
        launch {
            communicationTransport
                .submitResponse(response)
                .onSuccess {
                    stateMachine.onEvent(CommunicationStateEvent.ResponseSent(response))
                }
                .logFailure("Failed to submit response")
                .onFailure {
                    stateMachine.onEvent(CommunicationStateEvent.InvalidateSession(it))
                }
        }
    }

    private fun startPollingIncomingEvents() {
        incomingEventsPollingJob?.cancel()

        incomingEventsPollingJob = communicationTransport
            .subscribeIncoming()
            .onEach { eventsResult ->
                eventsResult
                    .onSuccess(::handleIncomingEvents)
                    .onFailure { stateMachine.onEvent(CommunicationStateEvent.InvalidateSession(it)) }
            }
            .launchIn(this@RealCommunicationSession)
    }

    private fun handleIncomingEvents(events: List<StatementTransportEvent>) {
        for (event in events) {
            when (event) {
                is StatementTransportEvent.Request -> {
                    stateMachine.onEvent(CommunicationStateEvent.RequestReceived(event))
                }

                is StatementTransportEvent.Response -> {
                    stateMachine.onEvent(CommunicationStateEvent.ResponseReceived(event))
                }
            }
        }
    }

    private fun stopPollingIncomingEvents() {
        incomingEventsPollingJob?.cancel()
        incomingEventsPollingJob = null
    }

    private fun handleIncomingRequest(request: StatementTransportEvent.Request) = launch {
        eventsFlow.emit(CommunicationSessionEvent.NewMessagesReceived(request.requestId, request.messages))
    }

    private fun handleIncomingResponse(messages: List<EncodedMessage>) = launch {
        eventsFlow.emit(CommunicationSessionEvent.ResponseReceived(messages))
    }

    private fun compactMessages(messages: List<EncodedMessage>) = launch {
        checkNotNull(compactor) { "Compact side effect emitted without a compactor" }
            .compact(messages)
            .logFailure("Failed to compact messages")
            .onSuccess { batches ->
                stateMachine.onEvent(CommunicationStateEvent.CompactionCompleted(batches))
            }
            .onFailure {
                stateMachine.onEvent(CommunicationStateEvent.CompactionFailed)
            }
    }

    private fun handleMessagesCompacted(commit: EncodedMessage, originals: List<EncodedMessage>) = launch {
        eventsFlow.emit(CommunicationSessionEvent.MessagesCompacted(commit, originals))
    }

    private fun handleTooLargeMessage(message: EncodedMessage, maxAllowedSize: InformationSize) = launch {
        eventsFlow.emit(CommunicationSessionEvent.MessageIsTooLarge(message, maxAllowedSize))
    }

    override fun subscribeEvents() = eventsFlow.asSharedFlow()

    override fun sendMessage(message: EncodedMessage) {
        stateMachine.onEvent(CommunicationStateEvent.SubmitMessage(message))
    }

    override fun encrypt(data: ByteArray): ByteArray {
        return encryption.encrypt(data)
    }

    override fun generateSharedIncomingSessionValue(salt: ByteArray): ByteArray {
        return generateSharedSessionValue(salt, incomingSessionIdParams, encryption)
    }

    override fun generateSharedOutgoingSessionValue(salt: ByteArray): ByteArray {
        return generateSharedSessionValue(salt, outgoingSessionIdParams, encryption)
    }
}
