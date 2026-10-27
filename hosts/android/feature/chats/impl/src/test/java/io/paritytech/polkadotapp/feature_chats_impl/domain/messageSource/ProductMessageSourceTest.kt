package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds

/** The window of a product-source room, as the iOS `ProductMessageSourceTests`. */
class ProductMessageSourceTest {
    // region Pages

    @Test
    fun `the room opens on the newest page with the newest message at the bottom`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 120)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying)

        assertEquals(history.ids(70..119), source.oldestFirstIds())
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)
        assertEquals(ChatPagingState(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.END), source.snapshot.paging)
        assertEquals(listOf(ChatPageAnchorModel.Before(null)), querying.pageRequests)
    }

    @Test
    fun `scrolling up pages with the returned cursor until the end and asks no page twice`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 120)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying)

        assertEquals(ChatPageOutcome.Appended(50), source.loadOlder())
        assertEquals(ChatPageOutcome.Appended(20), source.loadOlder())
        assertEquals(ChatPageOutcome.End, source.loadOlder())

        assertEquals(history.ids(0..119), source.oldestFirstIds())
        assertEquals(ChatPagingState.Edge.END, source.snapshot.paging.older)
        assertEquals(listOf(before(null), before("70"), before("20")), querying.pageRequests)
    }

    @Test
    fun `the window stays within 500 messages and a dropped page comes back with its own request`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 2_000)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying)

        repeat(20) {
            source.loadOlder()
            assertTrue(source.snapshot.messages.size <= 500)
        }

        assertEquals(ChatPagingState.Edge.MORE, source.snapshot.paging.newer)
        assertEquals(history.ids(950..1_449), source.oldestFirstIds())

        val requestsBefore = querying.pageRequests.size
        while (source.loadNewer() is ChatPageOutcome.Appended) {
            assertTrue(source.snapshot.messages.size <= 500)
        }

        assertEquals(history.ids(1_500..1_999), source.oldestFirstIds())
        assertEquals(ChatPagingState(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.END), source.snapshot.paging)
        val expected = (1_500..1_950 step 50).map { before(it.toString()) } + before(null)
        assertEquals(expected, querying.pageRequests.drop(requestsBefore))
    }

    @Test
    fun `messages that arrived while the newest page was out of the window come back without a gap`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 600)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        repeat(10) { source.loadOlder() }
        assertEquals(ChatPagingState.Edge.MORE, source.snapshot.paging.newer)

        history.append(30)
        while (source.loadNewer() is ChatPageOutcome.Appended) Unit

        val ids = source.oldestFirstIds()
        assertEquals("m629", ids.last())
        assertEquals(history.ids(630 - ids.size..629), ids)
    }

    @Test
    fun `a jump pages until the message is in the window and fails only when both ends come first`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 2_000)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        assertTrue(source.jump("m10"))
        assertTrue("m10" in source.oldestFirstIds())
        assertTrue(source.snapshot.messages.size <= 500)

        assertFalse(source.jump("missing"))
        assertEquals(ChatPagingState(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.END), source.snapshot.paging)
    }

    @Test
    fun `an older page that fails shows nothing and is asked again when the user comes near it`() = runBlocking<Unit> {
        val failures = AtomicInteger(1)
        val querying = ScriptedProductChatQuerying(answerPage = { anchor, _ ->
            when {
                anchor == before(null) -> newestPage(next = "1")
                failures.getAndDecrement() > 0 -> Result.failure(ProductChatQueryException.TimedOut())
                else -> Result.success(ProductMessagesPage(listOf(productMessage(0)), next = null))
            }
        })
        val source = openedSource(querying)

        assertEquals(ChatPageOutcome.Failed, source.loadOlder())

        // The list has no failure row, so the end can load again.
        assertEquals(2, querying.pageRequests.size)
        assertEquals(ChatPagingState.Edge.MORE, source.snapshot.paging.older)
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)

        assertEquals(ChatPageOutcome.Appended(1), source.loadOlder())
        assertEquals(listOf("m0", "m1"), source.oldestFirstIds())
    }

    @Test
    fun `a newest page that times out keeps the cached page and is asked again silently`() = runBlocking<Unit> {
        val cache = ProductMessageCache()
        cache.store(productRoom, ProductRoomCacheEntry(listOf(productMessage(1)), "1", "m1", 1_000, null))
        val answers = ArrayDeque(listOf(Result.failure<ProductMessagesPage>(ProductChatQueryException.TimedOut()), newestPage(next = null)))
        val querying = ScriptedProductChatQuerying(answerPage = { _, _ -> answers.removeFirst() })

        val source = openedSource(querying, cache = cache)

        assertEquals(listOf("m1"), source.oldestFirstIds())
        waitFor { source.snapshot.readiness.takeIf { it == ChatRoomReadiness.READY } }
        assertEquals(2, querying.pageRequests.size)
    }

    @Test
    fun `NotAvailable ends the older pages after one retry`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying(answerPage = { anchor, _ ->
            if (anchor == before(null)) newestPage(next = "1") else Result.failure(ProductChatQueryException.NotAvailable())
        })
        val source = openedSource(querying)

        assertEquals(ChatPageOutcome.End, source.loadOlder())

        assertEquals(3, querying.pageRequests.size)
        assertEquals(ChatPagingState.Edge.END, source.snapshot.paging.older)
    }

    @Test
    fun `a product that does not answer gets one retry`() = runBlocking<Unit> {
        val attempts = AtomicInteger()
        val querying = ScriptedProductChatQuerying(answerPage = { _, _ ->
            if (attempts.incrementAndGet() == 1) Result.failure(IllegalStateException("the worker stopped")) else newestPage(next = null)
        })
        val source = openedSource(querying)

        assertEquals(2, querying.pageRequests.size)
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)
        assertEquals(listOf("m1"), source.oldestFirstIds())
    }

    @Test
    fun `a room whose product does not answer keeps its cached page and is unavailable`() = runBlocking<Unit> {
        val cache = ProductMessageCache()
        cache.store(productRoom, ProductRoomCacheEntry(listOf(productMessage(1), productMessage(2)), "1", "m2", 2_000, null))
        val querying = ScriptedProductChatQuerying(answerPage = { _, _ -> Result.failure(ProductChatQueryException.NoAnswer()) })

        val source = openedSource(querying, cache = cache)

        assertEquals(ChatRoomReadiness.UNAVAILABLE, source.snapshot.readiness)
        assertEquals(listOf("m1", "m2"), source.oldestFirstIds())
        assertTrue(querying.pageRequests.size >= 2)
        // The newest page is asked again later, with a delay that grows.
        waitFor { querying.pageRequests.size.takeIf { it >= 4 } }
        source.close()
    }

    @Test
    fun `the cache keeps the newest page for the next open`() = runBlocking<Unit> {
        val cache = ProductMessageCache()
        val history = ProductRoomHistory(count = 80)
        openedSource(ScriptedProductChatQuerying(answerPage = history.answer), cache = cache)

        val second = source(ScriptedProductChatQuerying(answerPage = { _, _ -> Result.failure(ProductChatQueryException.NoAnswer()) }), cache = cache)
        second.start()

        assertEquals(history.ids(30..79), second.oldestFirstIds())
    }

    // endregion

    // region Feed updates

    @Test
    fun `new, edit, replace, reactions and status changes apply to the window`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 10)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))
        val edited = ChatMessage.Content.Text("edited")

        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(10)))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(45, id = "late", timestamp = 4_500)))
        source.apply(productRoom, ProductFeedUpdateModel.Edited("m3", edited, editedAt = 99))
        source.apply(productRoom, ProductFeedUpdateModel.Replaced(productMessage(2).copy(content = ChatMessage.Content.Text("replaced"))))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "a", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "b", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionRemoved("m1", "b", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "user", "🔥"))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(11, participant = "user")))
        source.apply(productRoom, ProductFeedUpdateModel.StatusChanged("m11", ProductMessageStatusModel.READ))

        val snapshot = source.snapshot
        assertEquals(listOf("m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9", "m10", "late", "m11"), source.oldestFirstIds())
        assertEquals(edited, snapshot.messages.single { it.id == "m3" }.content)
        assertEquals("replaced", (snapshot.messages.single { it.id == "m2" }.content as ChatMessage.Content.Text).text)
        assertFalse("m2" in snapshot.revisions)
        assertEquals(99L, snapshot.revisions.getValue("m3").timestamp)
        assertEquals(listOf("👍", "🔥"), snapshot.reactions.getValue("m1").map { it.content.emoji }.sorted())
        assertTrue(snapshot.reactions.getValue("m1").single { it.content.emoji == "🔥" }.origin is ChatMessageOrigin.User)
        assertEquals(ChatMessage.Status.IS_READ, snapshot.messages.single { it.id == "m11" }.status)
        assertTrue(snapshot.messages.first().origin is ChatMessageOrigin.User)
    }

    @Test
    fun `an incoming message is unread until the product reads it or the user sees it`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 3)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(3, status = ProductMessageStatusModel.DELIVERED)))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(4, status = ProductMessageStatusModel.READ)))
        assertEquals(
            listOf(
                ChatMessage.Status.NEW,
                ChatMessage.Status.NEW,
                ChatMessage.Status.NEW,
                ChatMessage.Status.NEW,
                ChatMessage.Status.IS_READ,
            ),
            source.snapshot.messages.reversed().map { it.status },
        )

        source.perform(ChatSourceAction.MarkSeen(setOf("m3")))

        assertTrue(source.snapshot.messages.all { it.status == ChatMessage.Status.IS_READ })
    }

    @Test
    fun `a feed update for another room is ignored`() = runBlocking<Unit> {
        val source = openedSource(ScriptedProductChatQuerying())

        source.apply(ChatId.forExtensionRoom("ProductBot_weather", "other"), ProductFeedUpdateModel.New(productMessage(1)))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(2)))

        assertEquals(listOf("m2"), source.oldestFirstIds())
    }

    // endregion

    // region User actions

    @Test
    fun `after a send the room shows its newest messages, and asks them again only when they are out of the window`() =
        runBlocking<Unit> {
            val history = ProductRoomHistory(count = 2_000)
            val querying = ScriptedProductChatQuerying(answerPage = history.answer)
            val source = openedSource(querying)

            assertEquals(ChatSourceActionOutcome.Done, source.perform(ChatSourceAction.ShowNewest))
            assertEquals(1, querying.pageRequests.size)

            repeat(20) { source.loadOlder() }
            assertEquals(ChatPagingState.Edge.MORE, source.snapshot.paging.newer)
            source.perform(ChatSourceAction.ShowNewest)

            assertEquals(history.ids(1_950..1_999), source.oldestFirstIds())
            assertTrue(querying.actionRequests.isEmpty())
        }

    @Test
    fun `a refused edit restores the body`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 5, participant = "user")
        val querying = ScriptedProductChatQuerying(
            answerPage = history.answer,
            answerAction = { Result.failure(ProductChatQueryException.Rejected("Locked")) },
        )
        val source = openedSource(querying)
        val original = source.snapshot.messages.single { it.id == "m3" }.content

        val outcome = source.perform(ChatSourceAction.Edit("m3", text("new")))

        assertEquals(ChatSourceActionOutcome.Refused("Locked"), outcome)
        assertTrue(querying.actionRequests.single() is ChatUserActionModel.EditMessage)
        assertEquals(original, source.snapshot.messages.single { it.id == "m3" }.content)
        assertFalse("m3" in source.snapshot.revisions)
    }

    @Test
    fun `an edit of a message with media keeps the media and takes the text as caption`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying()
        val source = openedSource(querying)
        val media = ChatMediaItemModel.Image(
            ChatResourceModel.Hop(byteArrayOf(1).toDataByteArray(), byteArrayOf(2).toDataByteArray(), "wss://hop.example", size = 1),
            ChatMediaMetaModel("image/png", sizeBytes = 1, width = 1, height = 1, placeholder = null, alt = null),
        )
        val photo = productMessage(1, participant = "user").copy(content = ChatMessage.Content.ProductBody(ProductMessageBody.RichText("old", listOf(media))))
        source.apply(productRoom, ProductFeedUpdateModel.New(photo))

        source.perform(ChatSourceAction.Edit("m1", text("new")))

        val body = (source.snapshot.messages.single().content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
        assertEquals("new", body.text)
        assertEquals(listOf(media), body.media)
        assertEquals(ChatUserActionModel.EditMessage("m1", text("new")).messageId, (querying.actionRequests.single() as ChatUserActionModel.EditMessage).messageId)
    }

    @Test
    fun `as in a person chat, a new emoji replaces the reaction of the user`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 2)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        source.perform(ChatSourceAction.ToggleReaction("m1", "👍"))
        source.perform(ChatSourceAction.ToggleReaction("m1", "🔥"))

        assertEquals(listOf("🔥"), source.snapshot.reactions.getValue("m1").map { it.content.emoji })
    }

    @Test
    fun `a reaction toggles the reaction of the user`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 2)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying)

        source.perform(ChatSourceAction.ToggleReaction("m1", "👍"))
        assertTrue(source.snapshot.reactions.getValue("m1").single().origin is ChatMessageOrigin.User)

        source.perform(ChatSourceAction.ToggleReaction("m1", "👍"))
        assertFalse("m1" in source.snapshot.reactions)

        assertEquals(
            listOf(ChatUserActionModel.ReactionPlaced("m1", "👍"), ChatUserActionModel.ReactionRemoved("m1", "👍")),
            querying.actionRequests,
        )
    }

    @Test
    fun `seen goes to the product for a newer message only`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 10)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying, reportsSeen = true)

        source.perform(ChatSourceAction.MarkSeen(setOf("m4", "m9")))
        waitFor { querying.actionRequests.firstOrNull() }
        source.perform(ChatSourceAction.MarkSeen(setOf("m5")))
        source.perform(ChatSourceAction.MarkSeen(setOf("m9")))
        delay(50)

        assertEquals(listOf(ChatUserActionModel.Seen("m9")), querying.actionRequests)
    }

    @Test
    fun `seen stays in the host when the product does not ask for it`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 10)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying, reportsSeen = false)

        source.perform(ChatSourceAction.MarkSeen(setOf("m9")))
        delay(50)

        assertTrue(querying.actionRequests.isEmpty())
    }

    // endregion

    private fun source(
        querying: ScriptedProductChatQuerying,
        cache: ProductMessageCaching = ProductMessageCache(),
        reportsSeen: Boolean = false,
    ) = ProductMessageSource(
        chatId = productRoom,
        querying = querying,
        cache = cache,
        settings = ProductMessageSource.Settings.Default.copy(retryDelay = 1.milliseconds, reportsSeen = reportsSeen),
        now = { 1_000_000L },
        dispatcher = Dispatchers.Default,
    )

    private suspend fun openedSource(
        querying: ScriptedProductChatQuerying,
        cache: ProductMessageCaching = ProductMessageCache(),
        reportsSeen: Boolean = false,
    ): ProductMessageSource = source(querying, cache, reportsSeen).apply {
        start()
        awaitFirstPage()
    }

    private fun ProductMessageSource.oldestFirstIds() = snapshot.messages.map { it.id }.reversed()

    private fun before(cursor: String?) = ChatPageAnchorModel.Before(cursor)

    private fun newestPage(next: String?) = Result.success(ProductMessagesPage(listOf(productMessage(1)), next = next))

    private fun text(text: String) = ChatMessage.Content.Text(text)

    private suspend fun <T : Any> waitFor(value: () -> T?): T = withTimeout(5_000) {
        var found = value()
        while (found == null) {
            delay(2)
            found = value()
        }
        found
    }
}
