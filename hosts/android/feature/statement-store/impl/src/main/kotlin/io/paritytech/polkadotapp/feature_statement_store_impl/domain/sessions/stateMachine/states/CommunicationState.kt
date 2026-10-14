package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.stateMachine.StateMachine
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationSideEffect
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.CommunicationStateEvent
import java.util.UUID

// topic size (32 byte) + channel size (32 byte) + expiry (8 byte) + proof (64 byte) + signer (32 byte)
private val STATEMENT_OVERHEAD = (32 + 32 + 8 + 64 + 32).bytes

internal val PROTOCOL_ACK_CODE: UByte = 0u

sealed class CommunicationState : StateMachine.State<CommunicationState, CommunicationSideEffect, CommunicationStateEvent> {
    abstract val maxStatementSize: InformationSize
    abstract val canCompact: Boolean

    protected val maxRequestSize: InformationSize get() = maxStatementSize - STATEMENT_OVERHEAD

    private val maxRequestSizeBytes: Long get() = maxRequestSize.inWholeBytes

    protected fun splitMessagesToFitRequest(messages: List<EncodedMessage>): MessagesSplit {
        val fitsInRequest = mutableListOf<EncodedMessage>()
        var currentSize = 0L

        for ((index, message) in messages.withIndex()) {
            if (currentSize + message.size <= maxRequestSizeBytes) {
                fitsInRequest.add(message)
                currentSize += message.size
            } else {
                val remaining = messages.subList(index, messages.size)
                return MessagesSplit(fitsInRequest, remaining)
            }
        }

        return MessagesSplit(fitsInRequest, emptyList())
    }

    protected fun generateNewRequestId(): String = UUID.randomUUID().toString()

    protected fun checkAlreadyPendingMessage(
        message: EncodedMessage,
        pendingMessages: List<EncodedMessage>,
        outgoingPendingRequest: StatementTransportEvent.Request?
    ): Boolean {
        if (pendingMessages.any { it.contentEquals(message) }) {
            return true
        }

        if (outgoingPendingRequest != null) {
            if (outgoingPendingRequest.messages.any { it.contentEquals(message) }) {
                return true
            }
        }

        return false
    }

    protected fun checkSizeLimitExceeded(message: EncodedMessage): Boolean {
        return message.size > maxRequestSizeBytes
    }

    protected data class MessagesSplit(
        val fitsInRequest: List<EncodedMessage>,
        val remaining: List<EncodedMessage>
    )
}
