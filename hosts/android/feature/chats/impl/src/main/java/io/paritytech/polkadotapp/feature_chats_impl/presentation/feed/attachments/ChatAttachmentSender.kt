package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.attachments

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.asAttachmentError
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.presentation.error.toPresentationError
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.AttachmentResult
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import timber.log.Timber

/**
 * Sends an attachment that the user picked. Before the first attachment that a product stages, the user sees the
 * public-storage notice, and the attachment waits for the answer (07-LATER §7.2).
 */
class ChatAttachmentSender(
    private val chatId: ChatId,
    private val interactor: ChatFeedInteractor,
    private val screen: Screen,
) {
    interface Screen {
        fun showNotice(productName: String)

        fun closeMenu()

        fun clearInput()

        fun showOutcome(outcome: ChatSourceActionOutcome)

        fun showFailure(error: PresentationThrowable)
    }

    private class Pending(val attachment: AttachmentResult, val text: String?, val replyToMessageId: ChatMessageId?)

    private var pending: Pending? = null

    /** The input keeps the caption while the notice shows, so Cancel loses nothing. */
    suspend fun send(attachment: AttachmentResult, text: String?, replyToMessageId: ChatMessageId?) {
        // The emulator launcher reads this line to know that the pick reached the room.
        Timber.i("truapi.chat.attachment picked %s %d B", attachment.mimeType, attachment.size.inWholeBytes)
        val noticeProduct = interactor.publicStorageNoticeFor(chatId, attachment.mimeType, attachment.size)
        if (noticeProduct != null) {
            pending = Pending(attachment, text, replyToMessageId)
            screen.showNotice(noticeProduct)
            return
        }

        screen.clearInput()
        deliver(Pending(attachment, text, replyToMessageId))
    }

    suspend fun acceptNotice() {
        val accepted = pending ?: return
        pending = null
        screen.closeMenu()

        interactor.acceptPublicStorageNotice(chatId)
        screen.clearInput()
        deliver(accepted)
    }

    /** The notice closed without an answer: the attachment is not sent. */
    fun dropPending() {
        pending = null
    }

    /** The failures of the attachments that the host stages for the room, as the room shows them. */
    fun observeFailures(): Flow<PresentationThrowable> =
        interactor.observeAttachmentFailures(chatId).map { it.toPresentationError() }

    private suspend fun deliver(attachment: Pending) {
        interactor.sendAttachmentMessage(
            chatId = chatId,
            contentUri = attachment.attachment.uri,
            mimeType = attachment.attachment.mimeType,
            fileSize = attachment.attachment.size,
            text = attachment.text,
            replyToMessageId = attachment.replyToMessageId
        ).onSuccess(screen::showOutcome).onFailure {
            screen.showFailure(it.asAttachmentError().toPresentationError())
        }
    }
}
