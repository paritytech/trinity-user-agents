package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatBuiltInMessageActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.inMenuOrder
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.isOutgoing
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.canSendMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.isPeerLeft
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
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

    /** [roomPolicy] is null for a person chat. */
    fun getMessageActions(
        message: ChatMessageUiModel,
        userInputState: ChatUserInputState,
        roomPolicy: ChatRoomPolicy?,
    ): List<AllowedMessageMenuAction> {
        return buildList {
            // An attachment of the outbox offers Retry in both room modes (07-LATER §7).
            val retries = roomPolicy?.offersRetry == true || ProductAttachmentOutbox.isOutboxId(message.id)
            if (roomPolicy != null && retries && message.status == ChatMessageUiModel.Status.FAILED && message.isOutgoing()) {
                add(AllowedMessageMenuAction.Retry)
            }

            userInputState.doWhenCanSendMessage {
                add(AllowedMessageMenuAction.Reply)
            }

            tryAddCopy(message)

            userInputState.doWhenCanSendMessage {
                tryAddEdit(message)
            }

            tryAddEditHistory(message)

            // A product room offers Delete on every message, a person chat never does (02-DESIGN §19 item 4).
            if (roomPolicy?.offersDelete == true) {
                add(AllowedMessageMenuAction.Delete)
            }
        }
    }

    fun canLeaveMenuReactions(userInputState: ChatUserInputState, roomPolicy: ChatRoomPolicy?): Boolean {
        val allowedReactions = roomPolicy?.messageMenu?.allowedReactions
        return userInputState.canSendMessage() && allowedReactions?.isEmpty() != true
    }

    /**
     * The entries of a message menu after the answer of `message_actions_query` (02-DESIGN §6.6). `Custom` gives only
     * the entries of the product. `ExtendBuiltIn` gives the host entries without `hidden`, then the entries of the
     * product. A destructive product entry goes last. No answer gives the host entries.
     */
    fun applyProductMenu(
        hostActions: List<AllowedMessageMenuAction>,
        menu: ChatMessageMenuModel?,
        icons: Map<String, Any>,
    ): List<AllowedMessageMenuAction> = when (menu) {
        null -> hostActions
        is ChatMessageMenuModel.Custom -> menu.actions.inMenuOrder().map { it.toMenuAction(icons) }
        is ChatMessageMenuModel.ExtendBuiltIn -> {
            val shown = hostActions.filter { action -> action.builtIn()?.let { it !in menu.hidden } ?: true }
            shown + menu.actions.inMenuOrder().map { it.toMenuAction(icons) }
        }
    }

    /** The emoji bar stays unless the product menu leaves out the reactions. */
    fun canLeaveReactions(hostAllows: Boolean, menu: ChatMessageMenuModel?): Boolean = when (menu) {
        null -> hostAllows
        is ChatMessageMenuModel.Custom -> false
        is ChatMessageMenuModel.ExtendBuiltIn -> hostAllows && ChatBuiltInMessageActionModel.REACT !in menu.hidden
    }

    private fun AllowedMessageMenuAction.builtIn(): ChatBuiltInMessageActionModel? = when (this) {
        AllowedMessageMenuAction.Reply -> ChatBuiltInMessageActionModel.REPLY
        is AllowedMessageMenuAction.Copy -> ChatBuiltInMessageActionModel.COPY
        is AllowedMessageMenuAction.Edit -> ChatBuiltInMessageActionModel.EDIT
        AllowedMessageMenuAction.Delete -> ChatBuiltInMessageActionModel.DELETE
        AllowedMessageMenuAction.Retry -> ChatBuiltInMessageActionModel.RETRY
        AllowedMessageMenuAction.ViewEditHistory,
        is AllowedMessageMenuAction.Custom -> null
    }

    private fun ChatProductActionModel.toMenuAction(icons: Map<String, Any>) =
        AllowedMessageMenuAction.Custom(actionId = actionId, label = label, icon = icons[actionId], isDestructive = destructive)

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
            is ChatMessageUiModel.Actions -> message.text
            is ChatMessageUiModel.ProductMedia -> message.text

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
            is ChatMessageUiModel.Text -> message.text.takeUnless { message.isMarkdown }
            is ChatMessageUiModel.Multimedia -> message.text

            is ChatMessageUiModel.Actions,
            is ChatMessageUiModel.ProductMedia,
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
