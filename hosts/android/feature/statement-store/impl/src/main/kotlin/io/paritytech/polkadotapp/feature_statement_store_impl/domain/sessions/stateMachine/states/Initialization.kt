package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.stateMachine.StateMachine
import io.paritytech.polkadotapp.feature_statement_store_api.domain.RequestId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationSideEffect
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationStateEvent
import kotlin.collections.orEmpty

class Initialization(
    private val pendingMessages: PendingMessages,
    override val maxStatementSize: InformationSize,
    override val canCompact: Boolean,
) : CommunicationState() {
    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    override suspend fun bootstrap() {
        transition.emitSideEffect(CommunicationSideEffect.FetchInitialData)
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    override suspend fun performTransition(event: CommunicationStateEvent) {
        when (event) {
            is CommunicationStateEvent.SubmitMessage -> {
                if (checkSizeLimitExceeded(event.message) && !canCompact) {
                    transition.emitSideEffect(CommunicationSideEffect.NotifyMessageTooLarge(event.message, maxRequestSize))
                    return
                }

                val placement = pendingMessages.placeNewMessage(event.message, requestMessages = emptyList())
                val newPendingMessages = when (placement) {
                    NewMessagePlacement.Skip -> return
                    is NewMessagePlacement.ReplacePending -> placement.pendingMessages
                    NewMessagePlacement.Append, is NewMessagePlacement.ReplaceInRequest -> pendingMessages + event.message
                }

                transition.emitState(
                    Initialization(
                        pendingMessages = newPendingMessages,
                        maxStatementSize = maxStatementSize,
                        canCompact = canCompact
                    )
                )
            }

            is CommunicationStateEvent.InitialDataFetched -> {
                var expiry = event.lastUsedExpiry
                val incomingRequests = mutableSetOf<RequestId>()

                event.incomingRequests.forEach { (request, isResponded) ->
                    if (!isResponded) {
                        expiry = StatementExpiry.nextAfter(expiry)

                        transition.emitSideEffect(CommunicationSideEffect.RequestReceived(request))
                        transition.emitSideEffect(
                            CommunicationSideEffect.SubmitResponse(
                                response = StatementTransportEvent.Response(
                                    requestId = request.requestId,
                                    expiry = expiry,
                                    responseCode = PROTOCOL_ACK_CODE
                                ),
                                request = request,
                            )
                        )
                    }

                    incomingRequests += request.requestId
                }

                if (pendingMessages.isEmpty()) {
                    transition.emitState(
                        activeState(event.outgoingPendingRequest, incomingRequests, pendingMessages, expiry, isCompacting = false)
                    )

                    transition.emitSideEffect(CommunicationSideEffect.StartPolling)
                    return
                }

                val alreadySentMessages = event.outgoingPendingRequest?.messages.orEmpty()
                val combinedMessages = alreadySentMessages + pendingMessages.messages
                val split = splitMessagesToFitRequest(combinedMessages)

                if (canCompact && split.remaining.isNotEmpty()) {
                    transition.emitState(
                        activeState(event.outgoingPendingRequest, incomingRequests, pendingMessages, expiry, isCompacting = true)
                    )
                    transition.emitSideEffect(CommunicationSideEffect.Compact(combinedMessages))
                    transition.emitSideEffect(CommunicationSideEffect.StartPolling)
                    return
                }

                val update = pendingMessages.applyReplacementsToRestoredRequest(alreadySentMessages)
                val updatedSplit = splitMessagesToFitRequest(update.requestMessages + update.pendingMessages.messages)
                val shouldCreateNewRequest = update.isRequestChanged || updatedSplit.fitsInRequest.size > alreadySentMessages.size

                if (shouldCreateNewRequest) {
                    createAndSubmitNewRequest(event.outgoingPendingRequest, incomingRequests, expiry, updatedSplit)
                } else {
                    transition.emitState(
                        activeState(event.outgoingPendingRequest, incomingRequests, update.pendingMessages, expiry, isCompacting = false)
                    )
                }

                transition.emitSideEffect(CommunicationSideEffect.StartPolling)
            }

            is CommunicationStateEvent.RequestSent -> Unit
            is CommunicationStateEvent.ResponseSent -> Unit
            is CommunicationStateEvent.ResponseReceived -> Unit
            is CommunicationStateEvent.RequestReceived -> Unit
            is CommunicationStateEvent.CompactionCompleted -> Unit
            is CommunicationStateEvent.CompactionFailed -> Unit
            is CommunicationStateEvent.InvalidateSession -> Unit
        }
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun createAndSubmitNewRequest(
        outgoingPendingRequest: StatementTransportEvent.Request?,
        incomingRequests: Set<RequestId>,
        expiry: ULong,
        split: MessagesSplit
    ) {
        val newExpiry = StatementExpiry.nextAfter(expiry)
        val newRequest = StatementTransportEvent.Request(
            requestId = generateNewRequestId(),
            expiry = newExpiry,
            messages = split.fitsInRequest
        )

        transition.emitState(
            activeState(newRequest, incomingRequests, pendingMessages.withMessages(split.remaining), newExpiry, isCompacting = false)
        )

        transition.emitSideEffect(
            CommunicationSideEffect.SubmitRequest(newRequest)
        )
    }

    private fun activeState(
        outgoingPendingRequest: StatementTransportEvent.Request?,
        incomingRequests: Set<RequestId>,
        pendingMessages: PendingMessages,
        expiry: ULong,
        isCompacting: Boolean
    ): Active {
        return Active(
            outgoingPendingRequest = outgoingPendingRequest,
            incomingRequests = incomingRequests,
            pendingMessages = pendingMessages,
            expiry = expiry,
            maxStatementSize = maxStatementSize,
            canCompact = canCompact,
            isCompacting = isCompacting
        )
    }
}
