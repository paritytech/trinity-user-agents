// This file is a temporary copy of the declarations contract of `feature/chats/api`.
// It has the package of the real types, so the merge deletes this file and changes no import.
package io.paritytech.polkadotapp.feature_chats_api.domain.model

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

data class ChatRoomDeclarations(
    val config: ChatRoomConfig,
    val metadata: ChatRoomMetadataDeclaration,
    val preview: ChatRoomPreview?,
    val toolbar: List<ChatRoomToolbarAction>,
    val suggestions: List<ChatRoomSuggestion>,
    val commands: List<ChatRoomCommand>,
    val footer: ChatRoomFooterConfig?,
)

data class ChatRoomConfig(
    val productSource: Boolean,
    val reactionDetails: Boolean,
    val participants: Boolean,
    val messageActions: Boolean,
    val participantActions: Boolean,
    val allowedReactions: List<String>?,
    val placeholder: String?,
    val reportSeen: Boolean,
) {
    companion object {
        val HostDefault = ChatRoomConfig(
            productSource = false,
            reactionDetails = false,
            participants = false,
            messageActions = false,
            participantActions = false,
            allowedReactions = null,
            placeholder = null,
            reportSeen = false,
        )
    }
}

data class ChatRoomMetadataDeclaration(
    val description: String?,
    val avatar: ChatResource?,
)

sealed interface ChatResource {
    data class Inline(val mimeType: String, val bytes: DataByteArray) : ChatResource

    data class Preimage(val key: DataByteArray) : ChatResource
}

data class ChatRoomPreview(
    val order: ChatRoomOrder?,
    val timestamp: Long?,
    val badge: ChatRoomBadge?,
    val text: ChatRoomPreviewText?,
)

enum class ChatRoomOrder { TIMESTAMP, PIN_TO_TOP }

sealed interface ChatRoomBadge {
    data class Count(val count: Int) : ChatRoomBadge

    data class Image(val resource: ChatResource) : ChatRoomBadge

    data object Hidden : ChatRoomBadge
}

data class ChatRoomPreviewText(val text: String, val ongoing: Boolean)

data class ChatRoomToolbarAction(
    val actionId: String,
    val label: String,
    val icon: ChatResource,
    val handler: ChatRoomActionHandler,
)

sealed interface ChatRoomActionHandler {
    data class Deeplink(val url: String) : ChatRoomActionHandler

    data object Callback : ChatRoomActionHandler
}

data class ChatRoomSuggestion(val suggestionId: String, val text: String, val sendsText: Boolean)

data class ChatRoomCommand(val name: String, val description: String, val argumentHint: String?)

sealed interface ChatRoomFooterConfig {
    data class Input(val config: ChatRoomInputConfig) : ChatRoomFooterConfig

    data object Empty : ChatRoomFooterConfig

    data object Custom : ChatRoomFooterConfig
}

data class ChatRoomInputConfig(
    val placeholder: String?,
    val photo: Boolean,
    val video: Boolean,
    val file: Boolean,
    val paymentDestination: DataByteArray?,
)

interface ChatRoomDeclarationsRepository {
    fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?>

    suspend fun get(chatId: ChatId): ChatRoomDeclarations?
}

class NoChatRoomDeclarationsRepository @Inject constructor() : ChatRoomDeclarationsRepository {
    override fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?> = flowOf(null)

    override suspend fun get(chatId: ChatId): ChatRoomDeclarations? = null
}

@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomDeclarationsContractModule {
    @Binds
    fun bindChatRoomDeclarationsRepository(impl: NoChatRoomDeclarationsRepository): ChatRoomDeclarationsRepository
}
