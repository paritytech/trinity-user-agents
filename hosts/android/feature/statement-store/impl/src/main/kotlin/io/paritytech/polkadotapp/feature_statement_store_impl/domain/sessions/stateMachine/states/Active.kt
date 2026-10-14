package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.stateMachine.StateMachine
import io.paritytech.polkadotapp.feature_statement_store_api.domain.RequestId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationSideEffect
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationStateEvent

data class Active(
    private val outgoingPendingRequest: StatementTransportEvent.Request?,
    private val incomingRequests: Set<RequestId>,
    private val pendingMessages: List<EncodedMessage>,
    private val expiry: ULong,
    override val maxStatementSize: InformationSize,
    override val canCompact: Boolean,
    private val isCompacting: Boolean,
) : CommunicationState() {
    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    override suspend fun performTransition(event: CommunicationStateEvent) {
        when (event) {
            is CommunicationStateEvent.InitialDataFetched -> Unit

            is CommunicationStateEvent.SubmitMessage -> {
                val exceedsSizeLimit = checkSizeLimitExceeded(event.message)

                if (exceedsSizeLimit && !canCompact) {
                    transition.emitSideEffect(CommunicationSideEffect.NotifyMessageTooLarge(event.message, maxRequestSize))
                    return
                }

                if (checkAlreadyPendingMessage(event.message, pendingMessages, outgoingPendingRequest)) return

                when {
                    isCompacting || pendingMessages.isNotEmpty() -> {
                        transition.emitState(
                            copy(pendingMessages = pendingMessages + event.message)
                        )
                    }

                    outgoingPendingRequest == null -> {
                        if (exceedsSizeLimit) {
                            startCompaction(
                                messagesToCompact = listOf(event.message),
                                newPendingMessages = listOf(event.message),
                                newOutgoingPendingRequest = null
                            )
                        } else {
                            submitPendingAsRequest(listOf(event.message))
                        }
                    }

                    else -> {
                        tryAppendMessageToOutgoingRequest(outgoingPendingRequest, event.message)
                    }
                }
            }

            is CommunicationStateEvent.CompactionCompleted -> {
                if (!isCompacting) return

                val inFlight = outgoingPendingRequest?.messages.orEmpty().minusByContent(event.allOriginals)
                val remainingPending = pendingMessages.minusByContent(event.allOriginals)
                val split = splitMessagesToFitRequest(inFlight + event.allCommits + remainingPending)
                val compactable = split.remaining.minusByContent(event.allCommits)

                if (canCompact && compactable.isNotEmpty()) {
                    submitSplitAndContinueCompacting(split, compactable)
                } else {
                    submitSplitAsRequest(split)
                }

                event.batches.forEach { batch ->
                    transition.emitSideEffect(
                        CommunicationSideEffect.MessagesCompacted(batch.commit, batch.originals)
                    )
                }
            }

            is CommunicationStateEvent.CompactionFailed -> {
                if (!isCompacting) return

                val (oversized, sendable) = pendingMessages.partition { checkSizeLimitExceeded(it) }

                oversized.forEach { message ->
                    transition.emitSideEffect(CommunicationSideEffect.NotifyMessageTooLarge(message, maxRequestSize))
                }

                when {
                    outgoingPendingRequest == null && sendable.isNotEmpty() -> {
                        submitPendingAsRequest(sendable)
                    }

                    outgoingPendingRequest != null && sendable.isNotEmpty() -> {
                        val split = splitMessagesToFitRequest(outgoingPendingRequest.messages + sendable)

                        if (split.fitsInRequest.size > outgoingPendingRequest.messages.size) {
                            submitSplitAsRequest(split)
                        } else {
                            transition.emitState(
                                copy(pendingMessages = sendable, isCompacting = false)
                            )
                        }
                    }

                    else -> {
                        transition.emitState(
                            copy(pendingMessages = sendable, isCompacting = false)
                        )
                    }
                }
            }

            is CommunicationStateEvent.RequestReceived -> {
                if (event.request.requestId in incomingRequests) return

                val newExpiry = StatementExpiry.nextAfter(expiry)

                transition.emitState(
                    copy(
                        incomingRequests = incomingRequests + event.request.requestId,
                        expiry = newExpiry
                    )
                )
                transition.emitSideEffect(
                    CommunicationSideEffect.RequestReceived(event.request)
                )
                transition.emitSideEffect(
                    CommunicationSideEffect.SubmitResponse(
                        StatementTransportEvent.Response(
                            requestId = event.request.requestId,
                            expiry = newExpiry,
                            responseCode = PROTOCOL_ACK_CODE
                        )
                    )
                )
            }

            is CommunicationStateEvent.ResponseReceived -> {
                if (event.response.requestId == outgoingPendingRequest?.requestId) {
                    transition.emitSideEffect(
                        CommunicationSideEffect.ResponseReceived(outgoingPendingRequest.messages)
                    )

                    if (isCompacting || pendingMessages.isEmpty()) {
                        transition.emitState(
                            copy(outgoingPendingRequest = null)
                        )
                        return
                    }

                    val split = splitMessagesToFitRequest(pendingMessages)

                    if (canCompact && split.remaining.isNotEmpty()) {
                        startCompaction(
                            messagesToCompact = pendingMessages,
                            newPendingMessages = pendingMessages,
                            newOutgoingPendingRequest = null
                        )
                    } else {
                        submitSplitAsRequest(split)
                    }
                }
            }

            is CommunicationStateEvent.RequestSent -> {
            }

            is CommunicationStateEvent.ResponseSent -> {
            }

            is CommunicationStateEvent.InvalidateSession -> {
                transition.emitSideEffect(CommunicationSideEffect.StopPolling)
                transition.emitState(Initialization(pendingMessages, maxStatementSize, canCompact))
                transition.emitSideEffect(CommunicationSideEffect.FetchInitialData)
            }
        }
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun tryAppendMessageToOutgoingRequest(
        outgoingRequest: StatementTransportEvent.Request,
        newMessage: EncodedMessage
    ) {
        check(pendingMessages.isEmpty())

        val currentMessages = outgoingRequest.messages
        val split = splitMessagesToFitRequest(currentMessages + newMessage)
        val hasAddedNewMessagesToRequest = split.fitsInRequest.size > currentMessages.size

        when {
            canCompact && split.remaining.isNotEmpty() -> {
                startCompaction(
                    messagesToCompact = currentMessages + newMessage,
                    newPendingMessages = listOf(newMessage),
                    newOutgoingPendingRequest = outgoingRequest
                )
            }

            hasAddedNewMessagesToRequest -> {
                submitSplitAsRequest(split)
            }

            else -> {
                transition.emitState(
                    copy(pendingMessages = listOf(newMessage))
                )
            }
        }
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun startCompaction(
        messagesToCompact: List<EncodedMessage>,
        newPendingMessages: List<EncodedMessage>,
        newOutgoingPendingRequest: StatementTransportEvent.Request?
    ) {
        transition.emitState(
            copy(
                outgoingPendingRequest = newOutgoingPendingRequest,
                pendingMessages = newPendingMessages,
                isCompacting = true
            )
        )

        transition.emitSideEffect(
            CommunicationSideEffect.Compact(messagesToCompact)
        )
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun submitPendingAsRequest(messages: List<EncodedMessage>) {
        submitSplitAsRequest(splitMessagesToFitRequest(messages))
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun submitSplitAsRequest(split: MessagesSplit) {
        val newExpiry = StatementExpiry.nextAfter(expiry)

        val request = StatementTransportEvent.Request(
            requestId = generateNewRequestId(),
            expiry = newExpiry,
            messages = split.fitsInRequest
        )

        transition.emitState(
            copy(
                outgoingPendingRequest = request,
                expiry = newExpiry,
                pendingMessages = split.remaining,
                isCompacting = false
            )
        )

        transition.emitSideEffect(
            CommunicationSideEffect.SubmitRequest(request)
        )
    }

    context(transition: StateMachine.Transition<CommunicationState, CommunicationSideEffect>)
    private suspend fun submitSplitAndContinueCompacting(split: MessagesSplit, messagesToCompact: List<EncodedMessage>) {
        val newExpiry = StatementExpiry.nextAfter(expiry)

        val request = StatementTransportEvent.Request(
            requestId = generateNewRequestId(),
            expiry = newExpiry,
            messages = split.fitsInRequest
        )

        transition.emitState(
            copy(
                outgoingPendingRequest = request,
                expiry = newExpiry,
                pendingMessages = split.remaining,
                isCompacting = true
            )
        )

        transition.emitSideEffect(
            CommunicationSideEffect.SubmitRequest(request)
        )
        transition.emitSideEffect(
            CommunicationSideEffect.Compact(messagesToCompact)
        )
    }
}

private val CommunicationStateEvent.CompactionCompleted.allOriginals: List<EncodedMessage>
    get() = batches.flatMap { it.originals }

private val CommunicationStateEvent.CompactionCompleted.allCommits: List<EncodedMessage>
    get() = batches.map { it.commit }

private fun List<EncodedMessage>.minusByContent(other: List<EncodedMessage>): List<EncodedMessage> {
    return filterNot { message -> other.any { it.contentEquals(message) } }
}
