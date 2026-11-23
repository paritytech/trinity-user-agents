package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.PendingMessageReplacementPolicy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.PendingMessageReplacementPolicy.Decision

data class PendingMessages(
    val messages: List<EncodedMessage>,
    private val replacementPolicy: PendingMessageReplacementPolicy,
) {
    fun isEmpty(): Boolean = messages.isEmpty()

    fun isNotEmpty(): Boolean = messages.isNotEmpty()

    fun withMessages(messages: List<EncodedMessage>): PendingMessages = copy(messages = messages)

    operator fun plus(message: EncodedMessage): PendingMessages = copy(messages = messages + message)

    fun placeNewMessage(message: EncodedMessage, requestMessages: List<EncodedMessage>): NewMessagePlacement {
        val queuedMessages = requestMessages + messages
        if (queuedMessages.any { it.contentEquals(message) }) return NewMessagePlacement.Skip

        return when (val outcome = outcomeAgainst(message, queuedMessages)) {
            ReplacementOutcome.Keep -> NewMessagePlacement.Append
            ReplacementOutcome.Drop -> NewMessagePlacement.Skip
            is ReplacementOutcome.Replace -> if (messages.any { it.contentEquals(outcome.replaced) }) {
                NewMessagePlacement.ReplacePending(withMessages(messages.replaceMessage(outcome.replaced, message)))
            } else {
                NewMessagePlacement.ReplaceInRequest(requestMessages.replaceMessage(outcome.replaced, message))
            }
        }
    }

    fun applyReplacementsToRestoredRequest(restoredRequestMessages: List<EncodedMessage>): RestoredRequestUpdate {
        var requestMessages = restoredRequestMessages
        var isRequestChanged = false
        val remainingMessages = mutableListOf<EncodedMessage>()

        for (message in messages) {
            when (val outcome = outcomeAgainst(message, requestMessages)) {
                ReplacementOutcome.Keep -> remainingMessages += message
                ReplacementOutcome.Drop -> Unit
                is ReplacementOutcome.Replace -> {
                    requestMessages = requestMessages.replaceMessage(outcome.replaced, message)
                    isRequestChanged = true
                }
            }
        }

        return RestoredRequestUpdate(requestMessages, withMessages(remainingMessages), isRequestChanged)
    }

    private fun outcomeAgainst(message: EncodedMessage, queuedMessages: List<EncodedMessage>): ReplacementOutcome {
        if (!replacementPolicy.isReplaceable(message)) return ReplacementOutcome.Keep

        val decisions = queuedMessages.map { queued -> queued to replacementPolicy.decide(message, queued) }
        if (decisions.any { (_, decision) -> decision == Decision.DROP_NEW }) return ReplacementOutcome.Drop

        val replaced = decisions.firstOrNull { (_, decision) -> decision == Decision.REPLACE_PENDING }?.first
        return if (replaced != null) ReplacementOutcome.Replace(replaced) else ReplacementOutcome.Keep
    }

    private fun List<EncodedMessage>.replaceMessage(replaced: EncodedMessage, message: EncodedMessage): List<EncodedMessage> {
        return map { if (it.contentEquals(replaced)) message else it }
    }

    private sealed interface ReplacementOutcome {
        object Keep : ReplacementOutcome
        object Drop : ReplacementOutcome
        class Replace(val replaced: EncodedMessage) : ReplacementOutcome
    }
}

sealed interface NewMessagePlacement {
    object Skip : NewMessagePlacement
    object Append : NewMessagePlacement
    class ReplacePending(val pendingMessages: PendingMessages) : NewMessagePlacement
    class ReplaceInRequest(val requestMessages: List<EncodedMessage>) : NewMessagePlacement
}

class RestoredRequestUpdate(
    val requestMessages: List<EncodedMessage>,
    val pendingMessages: PendingMessages,
    val isRequestChanged: Boolean,
)
