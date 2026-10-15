package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionButtonModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionsLayoutModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.ProductMedia.MediaItem
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProductBodyUiMapperTest {
    private val mapper = ProductBodyUiMapper()

    private val image = ChatResourceModel.Preimage(ByteArray(32) { 1 }.toDataByteArray())
    private val video = ChatResourceModel.Inline("video/mp4", byteArrayOf(1).toDataByteArray())

    @Test
    fun `an actions body keeps its buttons and layout`() = runBlocking<Unit> {
        val body = ProductMessageBody.Actions(
            text = "Pick one",
            actions = listOf(ChatActionButtonModel("yes", "Yes"), ChatActionButtonModel("no", "No")),
            layout = ChatActionsLayoutModel.GRID,
        )

        val result = map(body) as ChatMessageUiModel.Actions

        assertEquals("Pick one", result.text)
        assertEquals(
            listOf(ChatMessageUiModel.Actions.ActionButton("yes", "Yes"), ChatMessageUiModel.Actions.ActionButton("no", "No")),
            result.buttons,
        )
        assertEquals(ChatMessageUiModel.Actions.ActionsLayout.GRID, result.layout)
    }

    @Test
    fun `rich text without media is a markdown text message`() = runBlocking<Unit> {
        val result = map(ProductMessageBody.RichText(text = "**hi**", media = emptyList())) as ChatMessageUiModel.Text

        assertEquals("**hi**", result.text)
        assertEquals(true, result.isMarkdown)
    }

    @Test
    fun `rich text without text and media shows nothing`() = runBlocking<Unit> {
        assertNull(map(ProductMessageBody.RichText(text = " ", media = emptyList())))
    }

    @Test
    fun `media keep their resources so that the room screen loads them`() = runBlocking<Unit> {
        val body = ProductMessageBody.RichText(
            text = "Look",
            media = listOf(
                ChatMediaItemModel.Image(image, meta(width = 400, height = 300, alt = "A cat")),
                ChatMediaItemModel.Video(video, meta(width = 640, height = 360)),
                ChatMediaItemModel.File(image, meta(sizeBytes = 2048), fileName = "report.pdf"),
            ),
        )

        val result = map(body) as ChatMessageUiModel.ProductMedia

        assertEquals("Look", result.text)
        assertEquals(
            listOf(
                MediaItem.Image(resource = image, mimeType = "image/png", width = 400, height = 300, blurHash = "LKO2", alt = "A cat"),
                MediaItem.Video(resource = video, mimeType = "image/png", width = 640, height = 360, blurHash = "LKO2", alt = null),
                MediaItem.File(resource = image, mimeType = "image/png", fileName = "report.pdf", size = 2048L.bytes, alt = null),
            ),
            result.media,
        )
    }

    @Test
    fun `the product statuses map to the ticks`() {
        assertEquals(ChatMessageUiModel.Status.PENDING, ProductMessageStatusModel.PREPARING.toUi())
        assertEquals(ChatMessageUiModel.Status.SENT, ProductMessageStatusModel.SENT.toUi())
        assertEquals(ChatMessageUiModel.Status.DELIVERED, ProductMessageStatusModel.DELIVERED.toUi())
        assertEquals(ChatMessageUiModel.Status.READ, ProductMessageStatusModel.READ.toUi())
        assertEquals(ChatMessageUiModel.Status.FAILED, ProductMessageStatusModel.FAILED.toUi())
    }

    private fun map(body: ProductMessageBody) = mapper.map(
        message = message(body),
        body = body,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.SENT,
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false,
    )

    private fun message(body: ProductMessageBody) = ChatMessage(
        id = "message-1",
        chatId = ChatId.forExtensionRoom("ProductBot_weather", "berlin"),
        timestamp = 1L,
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        content = ChatMessage.Content.ProductBody(body),
        status = ChatMessage.Status.IS_SENT,
    )

    private fun meta(width: Int? = null, height: Int? = null, alt: String? = null, sizeBytes: Long? = null) = ChatMediaMetaModel(
        mimeType = "image/png",
        sizeBytes = sizeBytes,
        width = width,
        height = height,
        placeholder = "LKO2",
        alt = alt,
    )
}
