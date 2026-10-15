package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import android.content.Context
import androidx.compose.runtime.Immutable
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.toAvatar
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/** How the room screen draws a participant of a product room. [detail] names the kind of the participant. */
@Immutable
data class ChatParticipantUiModel(
    val participantId: String,
    val name: String,
    val detail: String,
    val avatar: AvatarUiModel,
)

/**
 * Names and draws the participants of a product room (02-DESIGN §3.3.7). The host has no lookup for a person or a
 * contact yet, so a person shows "Person #id", a lite person a short address and a contact a short handle (as iOS).
 */
class ChatParticipantAppearanceFactory @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val resourceResolver: ChatResourceResolver,
) {
    /**
     * [entry] is null for a reserved id or an id that the product did not resolve. [productOrigin] is the origin of the
     * messages of the product of the room.
     */
    suspend fun appearance(
        participantId: String,
        entry: ChatParticipantEntryModel?,
        origins: MessageOriginDisplayResolver,
        productOrigin: ChatMessageOrigin,
    ): ChatParticipantUiModel = when {
        // The host names the user "You", which needs no kind under it.
        participantId == ProductParticipant.USER -> origin(participantId, origins, ChatMessageOrigin.User, detail = "")
        participantId == ProductParticipant.PRODUCT ->
            origin(participantId, origins, productOrigin, context.getString(RCommon.string.chat_participant_kind_product))
        entry == null -> named(participantId, participantId, detail = "")
        else -> declared(entry)
    }

    private fun origin(participantId: String, origins: MessageOriginDisplayResolver, origin: ChatMessageOrigin, detail: String): ChatParticipantUiModel {
        val display = origins.displayOf(origin)
        return ChatParticipantUiModel(participantId, display.name, detail, display.avatar.toUi())
    }

    private suspend fun declared(entry: ChatParticipantEntryModel): ChatParticipantUiModel {
        val id = entry.participantId
        return when (val participant = entry.participant) {
            is ChatParticipantModel.Person -> named(
                id,
                context.getString(RCommon.string.chat_participant_person).format(participant.personId.toString()),
                context.getString(RCommon.string.chat_participant_kind_person),
            )

            is ChatParticipantModel.LitePerson ->
                named(id, participant.account.short(), context.getString(RCommon.string.chat_participant_kind_lite_person))

            is ChatParticipantModel.Contact -> named(
                id,
                context.getString(RCommon.string.chat_participant_contact).format(participant.handle.short()),
                context.getString(RCommon.string.chat_participant_kind_contact),
            )

            is ChatParticipantModel.Product -> named(id, participant.productId, context.getString(RCommon.string.chat_participant_kind_product))

            is ChatParticipantModel.Custom -> {
                val fallback = named(id, participant.nickname, context.getString(RCommon.string.chat_participant_kind_guest))
                val image = participant.avatar?.let { resourceResolver.resolve(it).getOrNull() }
                if (image == null) fallback else fallback.copy(avatar = image.toAvatar())
            }
        }
    }

    private fun named(participantId: String, name: String, detail: String) = ChatParticipantUiModel(
        participantId = participantId,
        name = name,
        detail = detail,
        avatar = AvatarUiModel.Name(name, AvatarColorScheme.from(participantId.encodeToByteArray())),
    )
}

/** The first and the last four hex digits of the bytes. */
internal fun DataByteArray.short(): String {
    val hex = value.joinToString(separator = "") { "%02x".format(it) }
    return if (hex.length > SHORT_HEX_LIMIT) "0x${hex.take(SHORT_HEX_PART)}…${hex.takeLast(SHORT_HEX_PART)}" else "0x$hex"
}

private const val SHORT_HEX_LIMIT = 12
private const val SHORT_HEX_PART = 4
