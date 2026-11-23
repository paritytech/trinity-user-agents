package io.paritytech.polkadotapp.feature_chats_impl.domain.seen

import io.paritytech.polkadotapp.feature_chats_impl.data.model.toTransientMessageOrNull
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.TransientMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.PendingMessageReplacementPolicy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.PendingMessageReplacementPolicy.Decision
import javax.inject.Inject

class KeepSingleSeenMessagePolicy @Inject constructor() : PendingMessageReplacementPolicy {
    override fun isReplaceable(message: EncodedMessage): Boolean {
        return message.toSeenOrNull() != null
    }

    override fun decide(newMessage: EncodedMessage, pendingMessage: EncodedMessage): Decision {
        val newSeen = newMessage.toSeenOrNull() ?: return Decision.KEEP_BOTH
        val pendingSeen = pendingMessage.toSeenOrNull() ?: return Decision.KEEP_BOTH

        val isSameContent = newSeen.messageId == pendingSeen.messageId && newSeen.timestamp == pendingSeen.timestamp
        return if (isSameContent) Decision.DROP_NEW else Decision.REPLACE_PENDING
    }

    private fun EncodedMessage.toSeenOrNull(): TransientMessage.Seen? {
        return toTransientMessageOrNull() as? TransientMessage.Seen
    }
}
