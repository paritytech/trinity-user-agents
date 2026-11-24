package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.IdentityProof
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.VersionedRequestContent
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.toDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementProofRemote

sealed interface IncomingChatRequestStatement {
    class Request(
        val requestId: ChatRequestId,
        val timestamp: Long,
        val proof: StatementProofRemote,
        val content: VersionedRequestContent.RequestVersion,
    ) : IncomingChatRequestStatement

    class Cancel(
        val requestId: ChatRequestId,
        val timestamp: Long,
        val identityProof: IdentityProof,
    ) : IncomingChatRequestStatement
}

fun ChatRequestDecrypted.toIncomingStatement(): IncomingChatRequestStatement {
    return when (val content = message.content) {
        is VersionedRequestContent.V1 -> IncomingChatRequestStatement.Request(message.messageId, message.timestamp.toLong(), proof, content)
        is VersionedRequestContent.V2 -> IncomingChatRequestStatement.Request(message.messageId, message.timestamp.toLong(), proof, content)
        is VersionedRequestContent.Cancel -> IncomingChatRequestStatement.Cancel(
            requestId = message.messageId,
            timestamp = message.timestamp.toLong(),
            identityProof = content.content.identityProof.toDomain(),
        )
    }
}
