package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel

/** A product entry of a message menu or a participant menu (`ChatProductAction`). */
data class ChatProductActionModel(
    val actionId: String,
    val label: String,
    val icon: ChatResourceModel?,
    /** The menu draws the entry in red, after the other entries. */
    val destructive: Boolean,
    val handler: ChatActionHandlerModel,
)

/** An entry of a message menu that the host makes itself (`ChatBuiltInMessageAction`). */
enum class ChatBuiltInMessageActionModel { COPY, REPLY, EDIT, DELETE, REACT, RETRY }

/** The menu of one message, as the product answers `message_actions_query` (`ChatMessageMenu`). */
sealed interface ChatMessageMenuModel {
    /** Only the entries of the product. */
    data class Custom(val actions: List<ChatProductActionModel>) : ChatMessageMenuModel

    /** The host entries without [hidden], then the entries of the product. */
    data class ExtendBuiltIn(
        val actions: List<ChatProductActionModel>,
        val hidden: Set<ChatBuiltInMessageActionModel>,
    ) : ChatMessageMenuModel
}

/** The menu of one participant, as the product answers `participant_actions_query` (`ChatParticipantMenu`). */
sealed interface ChatParticipantMenuModel {
    /** Only the entries of the product. The host entry of the participant goes away. */
    data class Custom(val actions: List<ChatProductActionModel>) : ChatParticipantMenuModel

    /** The host entry for the kind of the participant, then the entries of the product. */
    data class ExtendBuiltIn(val actions: List<ChatProductActionModel>) : ChatParticipantMenuModel
}

/** The entries in the order that a menu draws them: a destructive entry goes after the other entries. */
fun List<ChatProductActionModel>.inMenuOrder(): List<ChatProductActionModel> = filterNot { it.destructive } + filter { it.destructive }
