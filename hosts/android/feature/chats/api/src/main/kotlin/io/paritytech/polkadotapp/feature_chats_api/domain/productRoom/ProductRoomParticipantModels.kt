package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel

/** Who takes part in a product room (`ChatParticipant`, 02-DESIGN §3.3.7). */
sealed interface ChatParticipantModel {
    /** A verified person of the People chain. */
    data class Person(val personId: Long) : ChatParticipantModel

    /** A registered account that is not a person yet. [account] has 32 bytes. */
    data class LitePerson(val account: DataByteArray) : ChatParticipantModel

    /** A contact that the user picked through the Contacts API, by its opaque handle. */
    data class Contact(val handle: DataByteArray) : ChatParticipantModel

    /** Another product. */
    data class Product(val productId: String) : ChatParticipantModel

    /** Anyone else. The product gives the name and the avatar. */
    data class Custom(val nickname: String, val avatar: ChatResourceModel?) : ChatParticipantModel
}

/** A participant with the id that the product gives it in the room (`ChatParticipantEntry`). */
data class ChatParticipantEntryModel(val participantId: String, val participant: ChatParticipantModel)

/** One page of participants, in the order of the product. A null [next] is the last page. */
data class ProductParticipantsPage(val participants: List<ChatParticipantEntryModel>, val next: String?)
