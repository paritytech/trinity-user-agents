package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.DecodedChatPush
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.MESSAGE_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.PUSH_ID_KEY
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toChatPushContent
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealIncomingChatPushDecoder @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val encryptionFactory: CommunicationEncryption.Factory,
) : IncomingChatPushDecoder {
    override suspend fun decode(data: Map<String, String>): Result<DecodedChatPush> = runCatching {
        val pushId = data.getValue(PUSH_ID_KEY).fromHex().toDataByteArray()

        val contact = contactsRepository.getContactByPushId(pushId)
            ?: throw IllegalStateException("No contact found for pushId")

        if (contact.isBlocked) {
            throw IllegalStateException("Contact blocked")
        }

        val encryption = encryptionFactory.create(contact.sharedSecretDerivationDomain, contact.chatKey)
        val encrypted = data.getValue(MESSAGE_KEY).fromHex()
        val encoded = encryption.decrypt(encrypted)

        contact to encoded
    }.flatMap { (contact, encoded) ->
        encoded.toChatPushContent(
            authorAccountId = contact.accountId,
            contactAccountId = contact.accountId,
            messageStatus = ChatMessage.Status.NEW
        ).map { pushContent ->
            DecodedChatPush(
                contact = contact,
                chatId = ChatId.fromContact(contact.accountId),
                content = pushContent
            )
        }
    }
}
