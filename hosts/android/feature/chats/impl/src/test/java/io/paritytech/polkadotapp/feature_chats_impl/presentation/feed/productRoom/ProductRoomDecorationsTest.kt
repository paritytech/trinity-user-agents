package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBranchPositionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatQuoteText
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomDecorationsTest {
    private val extensionId = "ProductBot_sample.dot"
    private val room = ChatId.forExtensionRoom(extensionId, "host")
    private val allFeatures = listOf(ChatHostCapabilities.THREADS, ChatHostCapabilities.BRANCHES, ChatHostCapabilities.REASONING_ROWS)

    private val feed = FakeProductRoomAdditionsFeed()
    private val policy = MutableStateFlow<ChatRoomPolicy?>(ChatRoomPolicy.Defaults)
    private val snapshot = MutableStateFlow(snapshotOf(message("m1"), message("m2", ChatMessage.Content.Text("**Bold** and `code`"))))
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = MessageOriginDisplay("Sample", ChatAvatar.Url("https://sample"))
    }
    private val context = mockk<Context> { every { getString(any()) } returns "Bot" }
    private val resolver = mockk<ChatResourceResolver> {
        every { resolveIcon(any()) } returns Result.failure(IllegalStateException("no image in a unit test"))
    }

    @Test
    fun `the meta of a message draws its chip, branch control and reasoning row`() = runTest {
        feed.meta.value = mapOf(
            "m1" to ChatMessageMetaModel(thread = thread(), branch = ChatBranchPositionModel(1, 3), reasoning = null),
            "m2" to ChatMessageMetaModel(thread = null, branch = null, reasoning = ChatReasoningModel(durationMs = 4_000)),
        )
        val decorations = decorations()

        val drawn = decorations.decorations.first { it.size == 2 }

        val chip = drawn.getValue(stored("m1")).thread!!
        assertEquals(3L, chip.replyCount)
        assertEquals(ChatId.forExtensionRoom(extensionId, "thread-host-m1"), chip.threadChatId)
        assertEquals(listOf("p1", "Helper"), chip.repliers.map { it.name })
        assertEquals(BranchControlUiModel(index = 1, count = 3, waiting = false), drawn.getValue(stored("m1")).branch)
        assertEquals(ReasoningRowUiModel(thinking = false, durationMs = 4_000), drawn.getValue(stored("m2")).reasoning)
        assertTrue(decorations.isReasoning(stored("m2")))
    }

    @Test
    fun `a feature that the host does not list draws nothing`() = runTest {
        feed.meta.value = mapOf("m1" to ChatMessageMetaModel(thread = thread(), branch = ChatBranchPositionModel(1, 3), reasoning = null))
        val decorations = decorations(served = listOf(ChatHostCapabilities.BRANCHES))

        val drawn = decorations.decorations.first { it.isNotEmpty() }.getValue(stored("m1"))

        assertNull(drawn.thread)
        assertEquals(3, drawn.branch?.count)
    }

    @Test
    fun `a reasoning row thinks while its message is Preparing`() = runTest {
        feed.meta.value = mapOf("m2" to ChatMessageMetaModel(thread = null, branch = null, reasoning = ChatReasoningModel(durationMs = null)))
        feed.preparing.value = setOf("m2")
        val decorations = decorations()

        assertTrue(decorations.decorations.first { it.isNotEmpty() }.getValue(stored("m2")).reasoning!!.thinking)

        feed.preparing.value = emptySet()
        decorations.decorations.first { it[stored("m2")]?.reasoning?.thinking == false }
    }

    @Test
    fun `an arrow sends BranchSelected and waits until the product moves the position`() = runTest {
        feed.meta.value = mapOf("m1" to ChatMessageMetaModel(thread = null, branch = ChatBranchPositionModel(1, 3), reasoning = null))
        val decorations = decorations()
        decorations.decorations.first { it.isNotEmpty() }

        decorations.selectBranch(stored("m1"), 2)
        runCurrent()

        assertEquals(listOf(room to ChatRoomEventModel.BranchSelected("m1", 2)), feed.published)
        assertTrue(decorations.decorations.first { it.isNotEmpty() }.getValue(stored("m1")).branch!!.waiting)

        feed.meta.value = mapOf("m1" to ChatMessageMetaModel(thread = null, branch = ChatBranchPositionModel(2, 3), reasoning = null))
        runCurrent()

        assertEquals(BranchControlUiModel(index = 2, count = 3, waiting = false), decorations.decorations.value.getValue(stored("m1")).branch)
    }

    @Test
    fun `without an answer in 5 s the branch control takes the stored position again`() = runTest {
        feed.meta.value = mapOf("m1" to ChatMessageMetaModel(thread = null, branch = ChatBranchPositionModel(1, 3), reasoning = null))
        val decorations = decorations()
        decorations.decorations.first { it.isNotEmpty() }

        decorations.selectBranch(stored("m1"), 2)
        advanceTimeBy(4_900)
        assertTrue(decorations.decorations.value.getValue(stored("m1")).branch!!.waiting)

        advanceTimeBy(200)
        runCurrent()
        assertEquals(BranchControlUiModel(index = 1, count = 3, waiting = false), decorations.decorations.value.getValue(stored("m1")).branch)
    }

    @Test
    fun `a thread room shows the name of its parent and the root as plain text`() = runTest {
        feed.parentName = "Sample: host store"
        feed.root = ChatQuoteText("**Plain** text message", markdown = true)
        policy.value = ChatRoomPolicy.Defaults.copy(features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(threadOf = ChatThreadParentModel("host", "m1")))
        val decorations = decorations()

        assertEquals(ThreadRoomUiState(parentName = "Sample: host store", rootText = "Plain text message"), decorations.threadRoom.first { it != null })
    }

    @Test
    fun `read aloud speaks Text as it is and RichText without its marks`() = runTest {
        snapshot.value = snapshotOf(message("m1"), message("m2", ChatMessage.Content.RichText("**Bold** and `code`", emptyList())))
        val decorations = decorations()
        runCurrent()

        assertEquals("Hello", decorations.speakableText(stored("m1")))
        assertEquals("Bold and code", decorations.speakableText(stored("m2")))
        assertFalse(decorations.speakableText(stored("m2"))!!.contains("*"))
    }

    private fun TestScope.decorations(served: Collection<String> = allFeatures): ProductRoomDecorations {
        feed.personas.value = listOf(ChatBotPersonaModel("helper", "Helper", "data:image/png;base64,AA"))
        val participants = ProductRoomParticipants(
            scope = backgroundScope,
            chatId = room,
            roomPolicy = policy,
            feedSnapshot = snapshot,
            origins = flowOf(origins),
            appearances = ChatParticipantAppearanceFactory(context, resolver),
            personas = feed.personas,
            querying = { null },
        )
        return ProductRoomDecorations(backgroundScope, room, policy, snapshot, feed, participants, served)
    }

    private fun thread() = ChatThreadSummaryModel(
        roomId = "thread-host-m1",
        replyCount = 3,
        lastReplyAt = 1_000L,
        participantIds = listOf("p1", "bot:helper"),
    )

    private fun stored(productId: String) = ProductMessageIds.stored(productId, room)

    private fun message(productId: String, content: ChatMessage.Content = ChatMessage.Content.Text("Hello")) = ChatMessage(
        id = stored(productId),
        chatId = room,
        timestamp = 1_000L,
        origin = ChatMessageOrigin.Extension(extensionId),
        content = content,
        status = ChatMessage.Status.IS_READ,
    )

    private fun snapshotOf(vararg messages: ChatMessage) = ChatFeedSnapshot(
        messages = messages.toList(),
        reactions = emptyMap(),
        revisions = emptyMap(),
        productStatuses = emptyMap(),
        paging = ChatPagingState.Complete,
        readiness = ChatRoomReadiness.READY,
    )
}
