package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds

/** The window of a product-source room (04-TESTING S15 to S22, P8.5), as the iOS `ProductMessageSourceTests`. */
class ProductMessageSourceTest {
    // region Pages

    @Test
    fun `S15 the room opens on the newest page with the newest message at the bottom`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 120)
        val querying = ScriptedProductChatQuerying(answerPage = history.answer)
        val source = openedSource(querying)

        assertEquals(history.ids(70..119), source.oldestFirstIds())
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)
        assertEquals(ChatPagingState(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.END), source.snapshot.paging)
        assertEquals(listOf(ChatPageAnchorModel.Before(null)), querying.pageRequests)
    }

    @Test
    fun `S16 scrolling up pages with the returned cursor until the end and asks no page twice`() = runBlocking<Unit> {
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
    fun `S22 the window stays within 500 messages and a dropped page comes back with its own request`() = runBlocking<Unit> {
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
    fun `S17 a jump pages until the message is in the window and fails only when both ends come first`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 2_000)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        assertTrue(source.jump("m10"))
        assertTrue("m10" in source.oldestFirstIds())
        assertTrue(source.snapshot.messages.size <= 500)

        assertFalse(source.jump("missing"))
        assertEquals(ChatPagingState(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.END), source.snapshot.paging)
    }

    @Test
    fun `a page that times out fails without a retry`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying(answerPage = { anchor, _ ->
            if (anchor == before(null)) newestPage(next = "1") else Result.failure(ProductChatQueryException.TimedOut())
        })
        val source = openedSource(querying)

        assertEquals(ChatPageOutcome.Failed, source.loadOlder())

        assertEquals(2, querying.pageRequests.size)
        assertEquals(ChatPagingState.Edge.FAILED, source.snapshot.paging.older)
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)
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
    fun `S23 a room whose product does not answer keeps its cached page and is unavailable`() = runBlocking<Unit> {
        val cache = ProductMessageCache()
        cache.store(productRoom, ProductRoomCacheEntry(listOf(productMessage(1), productMessage(2)), "1", "m2", 2_000, null))
        val querying = ScriptedProductChatQuerying(answerPage = { _, _ -> Result.failure(ProductChatQueryException.NoAnswer()) })

        val source = openedSource(querying, cache = cache)

        assertEquals(ChatRoomReadiness.UNAVAILABLE, source.snapshot.readiness)
        assertEquals(listOf("m1", "m2"), source.oldestFirstIds())
        assertEquals(2, querying.pageRequests.size)
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
    fun `S18 new, edit, delete, reactions and status changes apply to the window`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 10)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))
        val edited = ChatMessage.Content.Text("edited")

        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(10)))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(45, id = "late", timestamp = 4_500)))
        source.apply(productRoom, ProductFeedUpdateModel.Edited("m3", edited, editedAt = 99))
        source.apply(productRoom, ProductFeedUpdateModel.Deleted("m2"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "a", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "b", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionRemoved("m1", "b", "👍"))
        source.apply(productRoom, ProductFeedUpdateModel.ReactionPlaced("m1", "user", "🔥"))
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(11, participant = "user")))
        source.apply(productRoom, ProductFeedUpdateModel.StatusChanged("m11", ProductMessageStatusModel.READ))

        val snapshot = source.snapshot
        assertEquals(listOf("m0", "m1", "m3", "m4", "late", "m5", "m6", "m7", "m8", "m9", "m10", "m11"), source.oldestFirstIds())
        assertEquals(edited, snapshot.messages.single { it.id == "m3" }.content)
        assertEquals(99L, snapshot.revisions.getValue("m3").timestamp)
        assertEquals(listOf("👍", "🔥"), snapshot.reactions.getValue("m1").map { it.content.emoji }.sorted())
        assertTrue(snapshot.reactions.getValue("m1").single { it.content.emoji == "🔥" }.origin is ChatMessageOrigin.User)
        assertEquals(ProductMessageStatusModel.READ, snapshot.productStatuses["m11"])
        assertTrue(snapshot.messages.first().origin is ChatMessageOrigin.User)
    }

    @Test
    fun `a live message is unread until the user sees it`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 3)
        val source = openedSource(ScriptedProductChatQuerying(answerPage = history.answer))

        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(3)))
        assertEquals(
            listOf(ChatMessage.Status.IS_READ, ChatMessage.Status.IS_READ, ChatMessage.Status.IS_READ, ChatMessage.Status.NEW),
            source.snapshot.messages.reversed().map { it.status },
        )

        source.perform(ChatSourceAction.MarkSeen(setOf("m3")))

        assertEquals(ChatMessage.Status.IS_READ, source.snapshot.messages.first().status)
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
    fun `S19 a send is optimistic until the product creates the message`() = runBlocking<Unit> {
        val gate = CompletableDeferred<Unit>()
        val created = productMessage(50, id = "created", participant = "user")
        val querying = ScriptedProductChatQuerying(answerAction = {
            gate.await()
            Result.success(ChatUserActionOutcome.MessageCreated(created))
        })
        val source = openedSource(querying)

        val send = async(Dispatchers.Default) { source.perform(ChatSourceAction.Send(text("hi"), replyTo = null)) }
        val optimistic = waitFor { source.snapshot.messages.firstOrNull { it.id.startsWith(ProductMessageSource.LOCAL_ID_PREFIX) } }
        assertEquals(ProductMessageStatusModel.PREPARING, source.snapshot.productStatuses[optimistic.id])
        assertEquals(ChatMessage.Status.NEW, optimistic.status)

        gate.complete(Unit)

        assertEquals(ChatSourceActionOutcome.Done, send.await())
        assertEquals(listOf("created"), source.oldestFirstIds())
        assertEquals(ProductMessageStatusModel.SENT, source.snapshot.productStatuses["created"])
        assertSentText("hi", replyTo = null, querying.actionRequests.single())
    }

    @Test
    fun `a reply carries the message it answers`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying()
        val source = openedSource(querying)

        source.perform(ChatSourceAction.Send(text("yes"), replyTo = "m1"))

        assertSentText("yes", replyTo = "m1", querying.actionRequests.single())
        assertEquals("m1", source.snapshot.messages.first().replyToMessageId)
    }

    @Test
    fun `S20 a send that the product rejects goes away with the reason`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying(answerAction = { Result.failure(ProductChatQueryException.Rejected("Too long")) })
        val source = openedSource(querying)

        val outcome = source.perform(ChatSourceAction.Send(text("hi"), replyTo = null))

        assertEquals(ChatSourceActionOutcome.Refused("Too long"), outcome)
        assertTrue(source.snapshot.messages.isEmpty())
    }

    @Test
    fun `S21 no answer in time marks the send failed and a retry sends it again`() = runBlocking<Unit> {
        val attempts = AtomicInteger()
        val created = productMessage(60, id = "created", participant = "user")
        val querying = ScriptedProductChatQuerying(answerAction = {
            if (attempts.incrementAndGet() == 1) {
                Result.failure(ProductChatQueryException.TimedOut())
            } else {
                Result.success(ChatUserActionOutcome.MessageCreated(created))
            }
        })
        val source = openedSource(querying)

        assertEquals(ChatSourceActionOutcome.Failed, source.perform(ChatSourceAction.Send(text("hi"), replyTo = null)))
        val failed = source.snapshot.messages.single()
        assertEquals(ProductMessageStatusModel.FAILED, source.snapshot.productStatuses[failed.id])
        assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)

        assertEquals(ChatSourceActionOutcome.Done, source.perform(ChatSourceAction.Retry(failed.id)))

        assertEquals(listOf("created"), source.oldestFirstIds())
        assertEquals(2, querying.actionRequests.size)
        querying.actionRequests.forEach { assertSentText("hi", replyTo = null, it) }
    }

    @Test
    fun `a retry of a failed product message asks the product`() = runBlocking<Unit> {
        val querying = ScriptedProductChatQuerying()
        val source = openedSource(querying)
        source.apply(productRoom, ProductFeedUpdateModel.New(productMessage(1, participant = "user", status = ProductMessageStatusModel.FAILED)))

        assertEquals(ChatSourceActionOutcome.Done, source.perform(ChatSourceAction.Retry("m1")))

        assertEquals(listOf(ChatUserActionModel.Retry("m1")), querying.actionRequests)
        assertEquals(ProductMessageStatusModel.PREPARING, source.snapshot.productStatuses["m1"])
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
    fun `a delete that times out puts the message back`() = runBlocking<Unit> {
        val history = ProductRoomHistory(count = 3)
        val querying = ScriptedProductChatQuerying(
            answerPage = history.answer,
            answerAction = { Result.failure(ProductChatQueryException.TimedOut()) },
        )
        val source = openedSource(querying)

        assertEquals(ChatSourceActionOutcome.Refused(null), source.perform(ChatSourceAction.Delete("m1")))

        assertEquals(listOf("m0", "m1", "m2"), source.oldestFirstIds())
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

    private fun assertSentText(text: String, replyTo: String?, request: ChatUserActionModel) {
        val newMessage = request as ChatUserActionModel.NewMessage
        assertEquals(text, (newMessage.content as ChatMessage.Content.Text).text)
        assertEquals(replyTo, newMessage.replyTo)
    }

    private suspend fun <T : Any> waitFor(value: () -> T?): T = withTimeout(5_000) {
        var found = value()
        while (found == null) {
            delay(2)
            found = value()
        }
        found
    }
}
