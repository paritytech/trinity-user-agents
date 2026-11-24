package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

interface PendingMessageReplacementPolicy {
    fun isReplaceable(message: EncodedMessage): Boolean

    fun decide(newMessage: EncodedMessage, pendingMessage: EncodedMessage): Decision

    enum class Decision { KEEP_BOTH, DROP_NEW, REPLACE_PENDING }

    companion object {
        val AlwaysKeep = object : PendingMessageReplacementPolicy {
            override fun isReplaceable(message: EncodedMessage) = false

            override fun decide(newMessage: EncodedMessage, pendingMessage: EncodedMessage) = Decision.KEEP_BOTH
        }
    }
}
