package io.paritytech.polkadotapp.feature_chats_impl.performance

import android.content.Context
import io.mockk.every
import io.mockk.excludeRecords
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageSource
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.ChatMenuActionsProvider
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ProductBodyUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantAppearanceFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomParticipants
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealChatMediaLoader
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicInteger
import kotlin.system.measureNanoTime

/**
 * The Android numbers of 04-TESTING B1 to B4 and B6 on the JVM. The suite runs only with `CHAT_V2_BENCH=1` and prints
 * `[bench]` lines; the numbers go to `results/RESULTS.md` of the plan.
 */
class ChatExtensionV2BenchmarkTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val room = ChatId.forExtensionRoom("ProductBot_bench.dot", "source")
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = MessageOriginDisplay("Bench", ChatAvatar.Url("https://bench"))
    }

    @Before
    fun onlyOnRequest() = assumeTrue("Set CHAT_V2_BENCH=1 to run the benchmarks", System.getenv("CHAT_V2_BENCH") == "1")

    // region B1

    @Test
    fun `B1 10000 custom participants paged at 100`() = runBlocking<Unit> {
        val participants = (0 until 10_000).map { ChatParticipantEntryModel("p$it", ChatParticipantModel.Custom(nickname = "Guest $it", avatar = null)) }
        val querying = object : ProductChatQuerying {
            override suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int) = error("not asked")
            override suspend fun userAction(chatId: ChatId, action: ChatUserActionModel) = error("not asked")
            override suspend fun participantsCount(chatId: ChatId) = Result.success(participants.size)
            override suspend fun participantsPage(chatId: ChatId, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> {
                val start = cursor?.toInt() ?: 0
                val end = minOf(start + pageSize, participants.size)
                return Result.success(ProductParticipantsPage(participants.subList(start, end), next = end.takeIf { it < participants.size }?.toString()))
            }
        }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val list = ProductRoomParticipants(
            scope = scope,
            chatId = room,
            roomPolicy = flowOf(ChatRoomPolicy.Defaults.copy(productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS))),
            feedSnapshot = flowOf(emptySnapshot()),
            origins = flowOf(origins),
            appearances = ChatParticipantAppearanceFactory(quietContext(), mockk<ChatResourceResolver>()),
            querying = { querying },
        )
        withTimeout(10_000) { list.uiState.first { it.declared } }

        val pageMillis = mutableListOf<Double>()
        val heapBefore = usedHeapMb()
        for (page in 1..100) {
            val nanos = measureNanoTime {
                list.loadMore()
                withTimeout(10_000) { list.uiState.first { it.list.participants.size == page * 100 && !it.list.loading } }
            }
            pageMillis += nanos / 1e6
        }
        val heapAfter = usedHeapMb()
        scope.cancel()

        report("B1 page of 100 appended, list to 10,000 (ms)", pageMillis)
        report("B1 first 10 pages (ms)", pageMillis.take(10))
        report("B1 last 10 pages (ms)", pageMillis.takeLast(10))
        println("[bench] B1 heap after GC: before %.1f MB, after 10,000 participants %.1f MB".format(heapBefore, heapAfter))
    }

    // endregion

    // region B2

    @Test
    fun `B2 a 50000 message product-source room`() = runBlocking<Unit> {
        val querying = HistoryQuerying(count = 50_000)

        val cold = mutableListOf<Double>()
        repeat(20) {
            val source = source(querying)
            cold += measureNanoTime {
                source.start()
                source.awaitFirstPage()
            } / 1e6
            assertEquals(ChatRoomReadiness.READY, source.snapshot.readiness)
            assertEquals(50, source.snapshot.messages.size)
            source.close()
        }
        report("B2 newest page visible, new source each time (ms)", cold)

        val source = source(querying)
        source.start()
        source.awaitFirstPage()
        val older = (1..40).map { measureNanoTime { source.loadOlder() } / 1e6 }
        report("B2 one older page of 50 (ms)", older)
        println("[bench] B2 window after 40 older pages: ${source.snapshot.messages.size} messages")
        assertTrue(source.snapshot.messages.size <= 500)

        val mapper = ChatMessageUiMapper(mockk(relaxed = true), ChatMenuActionsProvider(), ProductBodyUiMapper())
        val mapping = (1..20).map { measureNanoTime { mapper.map(source.snapshot.messages, origins, emptyMap(), emptyMap()) } / 1e6 }
        report("B2 map a window of ${source.snapshot.messages.size} messages to the screen (ms)", mapping)

        val jumpSource = source(querying)
        jumpSource.start()
        jumpSource.awaitFirstPage()
        val jump = measureNanoTime { assertTrue(jumpSource.jump("m${50_000 - 5_000}")) } / 1e6
        println("[bench] B2 jump to a message 5,000 back (100 pages): %.1f ms".format(jump))
        source.close()
        jumpSource.close()
    }

    // endregion

    // region B3

    @Test
    fun `B3 forty edits at 20 per second`() = runBlocking<Unit> {
        val querying = HistoryQuerying(count = 5_000)
        val source = source(querying)
        source.start()
        source.awaitFirstPage()
        repeat(9) { source.loadOlder() }
        println("[bench] B3 window: ${source.snapshot.messages.size} messages")

        val mapper = ChatMessageUiMapper(mockk(relaxed = true), ChatMenuActionsProvider(), ProductBodyUiMapper())
        val newest = "m4999"
        val seen = AtomicInteger()
        val mapMillis = mutableListOf<Double>()
        val screen = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        screen.launch {
            source.observeSnapshots().collect { snapshot ->
                mapMillis += measureNanoTime { mapper.map(snapshot.messages, origins, emptyMap(), emptyMap()) } / 1e6
                seen.incrementAndGet()
            }
        }
        delay(200)
        val before = seen.get()

        val apply = mutableListOf<Double>()
        repeat(40) { index ->
            val update = ProductFeedUpdateModel.Edited(newest, ChatMessage.Content.Text("streamed ${"x".repeat(index * 10)}"), editedAt = 10_000L + index)
            apply += measureNanoTime { source.apply(room, update) } / 1e6
            delay(50)
        }
        delay(300)
        screen.cancel()

        report("B3 apply one Edited to the window (ms)", apply)
        report("B3 map the window to the screen after an edit (ms)", mapMillis.drop(1))
        println("[bench] B3 snapshots that reached the screen for 40 edits: ${seen.get() - before}")
        source.close()

        // A screen that maps slower than the edits arrive sees fewer snapshots: the state flow keeps only the newest.
        val slowSource = source(querying)
        slowSource.start()
        slowSource.awaitFirstPage()
        val slowSeen = AtomicInteger()
        val slowScreen = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        slowScreen.launch { slowSource.observeSnapshots().collect { delay(120); slowSeen.incrementAndGet() } }
        delay(200)
        val slowBefore = slowSeen.get()
        repeat(40) { index ->
            slowSource.apply(room, ProductFeedUpdateModel.Edited(newest, ChatMessage.Content.Text("streamed $index"), editedAt = 20_000L + index))
            delay(50)
        }
        delay(400)
        slowScreen.cancel()
        println("[bench] B3 snapshots that a screen of 120 ms per frame saw for 40 edits: ${slowSeen.get() - slowBefore}")
        slowSource.close()
    }

    // endregion

    // region B4 and B6

    @Test
    fun `B4 a 1 MiB inline image to a file of the cache`() = runBlocking<Unit> {
        val loader = RealChatMediaLoader(mockk(), ChatResourceDiskCache(folder.newFolder("b4"), ChatResourceDiskCache.MAX_BYTES), TestCoroutineDispatchers(Dispatchers.IO))
        val first = (1..10).map { run ->
            val image = ChatResourceModel.Inline("image/png", ByteArray(1024 * 1024) { (it * 31 + run).toByte() }.toDataByteArray())
            measureNanoTime { loader.load(image, "png").getOrThrow() } / 1e6
        }
        val image = ChatResourceModel.Inline("image/png", ByteArray(1024 * 1024) { it.toByte() }.toDataByteArray())
        loader.load(image, "png").getOrThrow()
        val again = (1..10).map { measureNanoTime { loader.load(image, "png").getOrThrow() } / 1e6 }

        val digest = (1..10).map { measureNanoTime { MessageDigest.getInstance("SHA-256").digest(image.bytes.value) } / 1e6 }
        report("B4 first load of a 1 MiB inline image: hash and write (ms)", first)
        report("B4 SHA-256 of 1 MiB alone (ms)", digest)
        report("B4 load of a 1 MiB inline image that the cache has (ms)", again)
    }

    @Test
    fun `B6 the resource cache stays within 64 MiB and removes the least recently read`() {
        var clock = 0L
        val cache = ChatResourceDiskCache(folder.newFolder("b6"), ChatResourceDiskCache.MAX_BYTES, now = { 1_000_000L + 1_000 * clock++ })
        val item = ByteArray(1024 * 1024)

        val store = (0 until 100).map { index ->
            if (index >= 1) cache.file("item-0", null)
            measureNanoTime { cache.store("item-$index", null, item).getOrThrow() } / 1e6
        }
        val size = cache.sizeBytes()

        report("B6 store 1 MiB with up to 64 files in the cache (ms)", store)
        println("[bench] B6 cache after 100 MiB stored: %.1f MiB (bound 64 MiB)".format(size / 1024.0 / 1024.0))
        println("[bench] B6 the item read before each store stays: ${cache.file("item-0", null) != null}; item-1 removed: ${cache.file("item-1", null) == null}")
        assertTrue(size <= ChatResourceDiskCache.MAX_BYTES)
        val reads = (0 until 64).map { index -> measureNanoTime { cache.file("item-${99 - index}", null) } / 1e6 }
        report("B6 read a file of the cache (ms)", reads)
    }

    // endregion

    private fun source(querying: ProductChatQuerying) = ProductMessageSource(
        chatId = room,
        querying = querying,
        cache = ProductMessageCache(),
        settings = ProductMessageSource.Settings.Default,
        now = { 1L },
        dispatcher = Dispatchers.Default,
    )

    /** A product that keeps [count] messages, newest last, and answers `Before` pages at once. */
    private class HistoryQuerying(private val count: Int) : ProductChatQuerying {
        private val messages = (0 until count).map { index ->
            ProductRoomMessage(
                messageId = "m$index",
                participantId = if (index % 3 == 0) "user" else "product",
                timestamp = 1_000L + index,
                content = if (index % 2 == 0) {
                    ChatMessage.Content.Text("Message $index with some text")
                } else {
                    ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text = "**Rich** message $index", media = emptyList()))
                },
                status = ProductMessageStatusModel.SENT,
                replyTo = null,
                editedAt = null,
                reactions = emptyList(),
            )
        }

        override suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage> {
            val end = when (anchor) {
                is ChatPageAnchorModel.Before -> anchor.cursor?.toInt() ?: count
                is ChatPageAnchorModel.After -> error("the source asks no After page")
            }
            val start = maxOf(0, end - pageSize)
            return Result.success(ProductMessagesPage(messages.subList(start, end), next = start.takeIf { it > 0 }?.toString()))
        }

        override suspend fun userAction(chatId: ChatId, action: ChatUserActionModel): Result<ChatUserActionOutcome> =
            Result.success(ChatUserActionOutcome.Acknowledged)
    }

    private fun emptySnapshot() = ChatFeedSnapshot(
        messages = emptyList(),
        reactions = emptyMap(),
        revisions = emptyMap(),
        productStatuses = emptyMap(),
        paging = ChatPagingState.Complete,
        readiness = ChatRoomReadiness.READY,
    )

    // A recording mock keeps each call, which a benchmark of 10,000 participants must not measure.
    private fun quietContext(): Context = mockk<Context>().also { context ->
        every { context.getString(any()) } returns "Guest"
        excludeRecords { context.getString(any()) }
    }

    private fun usedHeapMb(): Double {
        repeat(3) { System.gc() }
        val runtime = Runtime.getRuntime()
        return (runtime.totalMemory() - runtime.freeMemory()) / 1024.0 / 1024.0
    }

    private fun report(label: String, millis: List<Double>) {
        val sorted = millis.sorted()
        fun at(fraction: Double) = sorted[((sorted.size - 1) * fraction).toInt()]
        println("[bench] $label: n=%d median %.2f p90 %.2f max %.2f".format(sorted.size, at(0.5), at(0.9), sorted.last()))
    }
}
