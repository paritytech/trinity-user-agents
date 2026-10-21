package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import uniffi.truapi.ChatParticipant
import uniffi.truapi.ChatParticipantEntry
import uniffi.truapi.ContactHandle
import uniffi.truapi.ProductChatParticipantsQueryItem

// Maps the participants of Chat Extension v2 (02-DESIGN §3.3.7) between the bindings and the chat models of the app.

fun ChatParticipantEntry.toModel() = ChatParticipantEntryModel(participantId, participant.toModel())

fun ChatParticipantModel.toCore(): ChatParticipant = when (this) {
    is ChatParticipantModel.Person -> ChatParticipant.Person(personId.toUInt())
    is ChatParticipantModel.LitePerson -> ChatParticipant.LitePerson(account.value)
    is ChatParticipantModel.Contact -> ChatParticipant.Contact(ContactHandle(handle.value))
    is ChatParticipantModel.Product -> ChatParticipant.Product(productId)
    is ChatParticipantModel.Custom -> ChatParticipant.Custom(nickname, avatar?.toCore())
}

fun ChatParticipantEntryModel.toCore() = ChatParticipantEntry(participantId, participant.toCore())

private fun ChatParticipant.toModel(): ChatParticipantModel = when (this) {
    is ChatParticipant.Person -> ChatParticipantModel.Person(personId.toLong())
    is ChatParticipant.LitePerson -> ChatParticipantModel.LitePerson(account.toDataByteArray())
    is ChatParticipant.Contact -> ChatParticipantModel.Contact(handle.bytes.toDataByteArray())
    is ChatParticipant.Product -> ChatParticipantModel.Product(productId)
    is ChatParticipant.Custom -> ChatParticipantModel.Custom(nickname, avatar?.toModel())
}

fun ProductChatParticipantsQueryItem.toCount(): Int =
    (this as? ProductChatParticipantsQueryItem.Count)?.count?.toInt() ?: throw otherAnswer()

fun ProductChatParticipantsQueryItem.toPage(): ProductParticipantsPage =
    (this as? ProductChatParticipantsQueryItem.Page)?.let { page -> ProductParticipantsPage(page.participants.map { it.toModel() }, page.next) }
        ?: throw otherAnswer()

fun ProductChatParticipantsQueryItem.toResolved(): List<ChatParticipantEntryModel> =
    (this as? ProductChatParticipantsQueryItem.Resolved)?.participants?.map { it.toModel() } ?: throw otherAnswer()

private fun otherAnswer() = ProductChatQueryException.Failed("the product answered another participants question")
