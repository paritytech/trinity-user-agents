package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.canSendMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.isPeerLeft
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatToolbarAction
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toPersistentList
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ChatMenuActionsProvider @Inject constructor() {
    fun getToolbarActions(chatRequest: OpenChatRequest, userInputState: ChatUserInputState, addMenuIconForcibly: Boolean): ImmutableList<ChatToolbarAction> {
        return when (chatRequest) {
            is OpenChatRequest.ExistingChat -> toolbarForExistingChat(chatRequest, userInputState, addMenuIconForcibly)
            is OpenChatRequest.StartChatWithContact -> toolbarForContact(userInputState)
        }
    }

    fun getChatMenuActions(userInputState: ChatUserInputState): ImmutableList<ChatMenuAction> {
        return buildList {
            add(ChatMenuAction.COPY_USERNAME)

            // Leave + Block only once established or peer-left; hidden while pending/blocked (matches iOS).
            if (userInputState.canSendMessage() || userInputState.isPeerLeft()) {
                add(ChatMenuAction.LEAVE_CHAT)
                add(ChatMenuAction.BLOCK_USER)
            }
        }.toPersistentList()
    }

    fun getMessageActions(message: ChatMessageUiModel, userInputState: ChatUserInputState): List<AllowedMessageMenuAction> {
        return buildList {
            userInputState.doWhenCanSendMessage {
                add(AllowedMessageMenuAction.Reply)
            }

            tryAddCopy(message)

            userInputState.doWhenCanSendMessage {
                tryAddEdit(message)
            }

            tryAddEditHistory(message)
        }
    }

    fun canLeaveMenuReactions(userInputState: ChatUserInputState): Boolean {
        return userInputState.canSendMessage()
    }

    private fun MutableList<AllowedMessageMenuAction>.tryAddCopy(message: ChatMessageUiModel) {
        val editableText = extractCopiableText(message) ?: return
        add(AllowedMessageMenuAction.Copy(editableText))
    }

    private fun MutableList<AllowedMessageMenuAction>.tryAddEdit(message: ChatMessageUiModel) {
        val editableText = extractEditableText(message) ?: return

        add(AllowedMessageMenuAction.Edit(editableText))
    }

    private fun MutableList<AllowedMessageMenuAction>.tryAddEditHistory(message: ChatMessageUiModel) {
        val isEditedTextMessage = message is ChatMessageUiModel.Text && message.isEdited
        val isEditedMultimediaMessage = message is ChatMessageUiModel.Multimedia && message.isEdited
        if (isEditedTextMessage || isEditedMultimediaMessage) {
            add(AllowedMessageMenuAction.ViewEditHistory)
        }
    }

    private fun extractCopiableText(message: ChatMessageUiModel): String? {
        return when (message) {
            is ChatMessageUiModel.Text -> message.text
            is ChatMessageUiModel.Multimedia -> message.text
            is ChatMessageUiModel.File -> message.text
            is ChatMessageUiModel.ChatRequest -> message.welcomeText

            is ChatMessageUiModel.ChatAccepted,
            is ChatMessageUiModel.ContactAdded,
            is ChatMessageUiModel.Custom<*>,
            is ChatMessageUiModel.CoinagePayment,
            is ChatMessageUiModel.Unsupported,
            is ChatMessageUiModel.CompactionUnavailable,
            is ChatMessageUiModel.Call -> null
        }
    }

    private fun extractEditableText(message: ChatMessageUiModel): String? {
        if (message.isIncoming()) return null

        return when (message) {
            is ChatMessageUiModel.Text -> message.text
            is ChatMessageUiModel.Multimedia -> message.text

            is ChatMessageUiModel.ChatRequest,
            is ChatMessageUiModel.File,
            is ChatMessageUiModel.ChatAccepted,
            is ChatMessageUiModel.ContactAdded,
            is ChatMessageUiModel.Custom<*>,
            is ChatMessageUiModel.CoinagePayment,
            is ChatMessageUiModel.Unsupported,
            is ChatMessageUiModel.CompactionUnavailable,
            is ChatMessageUiModel.Call -> null
        }
    }

    private fun ChatUserInputState.doWhenCanSendMessage(action: () -> Unit) {
        if (canSendMessage()) action()
    }

    private fun toolbarForExistingChat(
        chatRequest: OpenChatRequest.ExistingChat,
        userInputState: ChatUserInputState,
        addMenuIconForcibly: Boolean
    ): ImmutableList<ChatToolbarAction> {
        return when (chatRequest.chatId.chatVariant()) {
            is ChatVariant.Contact -> toolbarForContact(userInputState)
            is ChatVariant.Extension -> toolbarForExtension(addMenuIconForcibly)
        }
    }

    private fun toolbarForContact(userInputState: ChatUserInputState): ImmutableList<ChatToolbarAction> {
        return buildList {
            if (userInputState is ChatUserInputState.SendMessage) {
                add(ChatToolbarAction.AUDIO_CALL)
                add(ChatToolbarAction.VIDEO_CALL)
            }
            add(ChatToolbarAction.MENU)
        }.toPersistentList()
    }

    private fun toolbarForExtension(addMenuIconForcibly: Boolean): ImmutableList<ChatToolbarAction> {
        return buildList {
            if (addMenuIconForcibly) {
                add(ChatToolbarAction.MENU)
            }
        }.toPersistentList()
    }
}
