package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.attachments

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.AttachmentResult
import kotlinx.coroutines.runBlocking
import org.junit.Test

class ChatAttachmentSenderTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val attachment = AttachmentResult(mockk<Uri>(), "application/pdf", 3_000_000.bytes)
    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        coEvery { sendAttachmentMessage(any(), any(), any(), any(), any(), any()) } returns Result.success(ChatSourceActionOutcome.Done)
    }
    private val screen = mockk<ChatAttachmentSender.Screen>(relaxed = true)
    private val sender = ChatAttachmentSender(room, interactor, screen)

    @Test
    fun `the attachment waits for the notice, and the caption stays until the user accepts`() = runBlocking {
        coEvery { interactor.publicStorageNoticeFor(room, "application/pdf", any()) } returns "Sample"

        sender.send(attachment, "Report", "m0")

        verify { screen.showNotice("Sample") }
        verify(exactly = 0) { screen.clearInput() }
        coVerify(exactly = 0) { interactor.sendAttachmentMessage(any(), any(), any(), any(), any(), any()) }

        sender.acceptNotice()

        verify { interactor.acceptPublicStorageNotice(room) }
        verify { screen.clearInput() }
        coVerify { interactor.sendAttachmentMessage(room, attachment.uri, "application/pdf", any(), "Report", "m0") }
    }

    @Test
    fun `a closed notice sends nothing`() = runBlocking {
        coEvery { interactor.publicStorageNoticeFor(room, any(), any()) } returns "Sample"

        sender.send(attachment, null, null)
        sender.dropPending()
        sender.acceptNotice()

        verify(exactly = 0) { interactor.acceptPublicStorageNotice(any()) }
        coVerify(exactly = 0) { interactor.sendAttachmentMessage(any(), any(), any(), any(), any(), any()) }
    }

    @Test
    fun `without the notice the attachment goes at once`() = runBlocking {
        coEvery { interactor.publicStorageNoticeFor(room, any(), any()) } returns null

        sender.send(attachment, null, null)

        verify(exactly = 0) { screen.showNotice(any()) }
        verify { screen.clearInput() }
        coVerify { interactor.sendAttachmentMessage(room, attachment.uri, "application/pdf", any(), null, null) }
    }
}
