package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine

import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.stateMachine.StateMachine
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CompactedBatch
import io.paritytech.polkadotapp.feature_statement_store_api.domain.NOT_RESPONDED
import io.paritytech.polkadotapp.feature_statement_store_api.domain.RequestId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states.Active
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states.CommunicationState
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.stateMachine.states.Initialization
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// maxRequestSize = MAX_STATEMENT_SIZE - 168 bytes of statement overhead = 100 bytes
private val MAX_STATEMENT_SIZE = (168 + 100).bytes

class ActiveStateTest {
    private class RecordingTransition : StateMachine.Transition<CommunicationState, CommunicationSideEffect> {
        val states = mutableListOf<CommunicationState>()
        val sideEffects = mutableListOf<CommunicationSideEffect>()

        override suspend fun emitState(newState: CommunicationState) {
            states += newState
        }

        override suspend fun emitSideEffect(sideEffect: CommunicationSideEffect) {
            sideEffects += sideEffect
        }
    }

    private fun message(size: Int, fill: Byte): EncodedMessage = ByteArray(size) { fill }

    private fun request(vararg messages: EncodedMessage, requestId: RequestId = "request-1") =
        StatementTransportEvent.Request(requestId = requestId, expiry = 10uL, messages = messages.toList())

    private fun active(
        outgoingPendingRequest: StatementTransportEvent.Request? = null,
        incomingRequests: Set<RequestId> = emptySet(),
        pendingMessages: List<EncodedMessage> = emptyList(),
        canCompact: Boolean = true,
        isCompacting: Boolean = false,
    ) = Active(
        outgoingPendingRequest = outgoingPendingRequest,
        incomingRequests = incomingRequests,
        pendingMessages = pendingMessages,
        expiry = 10uL,
        maxStatementSize = MAX_STATEMENT_SIZE,
        canCompact = canCompact,
        isCompacting = isCompacting
    )

    private fun recordTransition(state: CommunicationState, event: CommunicationStateEvent): RecordingTransition = runBlocking {
        RecordingTransition().also { recorder ->
            with(recorder as StateMachine.Transition<CommunicationState, CommunicationSideEffect>) {
                state.performTransition(event)
            }
        }
    }

    private fun RecordingTransition.newState(): CommunicationState = states.last()

    private inline fun <reified T : CommunicationSideEffect> RecordingTransition.assertSingleEffect(): T {
        val matching = sideEffects.filterIsInstance<T>()
        assertEquals("Expected exactly one ${T::class.simpleName}", 1, matching.size)
        return matching.first()
    }

    private inline fun <reified T : CommunicationSideEffect> RecordingTransition.assertNone() {
        assertTrue(sideEffects.filterIsInstance<T>().isEmpty())
    }

    @Test
    fun `overflowing append with canCompact triggers compaction of outgoing request plus new message`() {
        val alreadySent = message(60, 1)
        val newMessage = message(60, 2)

        val recorder = recordTransition(
            active(outgoingPendingRequest = request(alreadySent)),
            CommunicationStateEvent.SubmitMessage(newMessage)
        )

        val compact = recorder.assertSingleEffect<CommunicationSideEffect.Compact>()
        assertEquals(listOf(alreadySent, newMessage), compact.messages)
        recorder.assertNone<CommunicationSideEffect.SubmitRequest>()
        recorder.assertNone<CommunicationSideEffect.NotifyMessageTooLarge>()
    }

    @Test
    fun `messages submitted during compaction are buffered without side effects`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val compacting = recordTransition(
            active(outgoingPendingRequest = request(alreadySent)),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()

        val recorder = recordTransition(compacting, CommunicationStateEvent.SubmitMessage(message(10, 3)))

        assertTrue(recorder.sideEffects.isEmpty())
    }

    @Test
    fun `compaction completed replaces outgoing request with commit and drains buffered messages`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val buffered = message(10, 3)
        val commit = message(20, 4)

        var state = recordTransition(
            active(outgoingPendingRequest = request(alreadySent)),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()
        state = recordTransition(state, CommunicationStateEvent.SubmitMessage(buffered)).newState()

        val recorder = recordTransition(
            state,
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(commit, listOf(alreadySent, trigger))))
        )

        val submit = recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>()
        assertEquals(listOf(commit, buffered), submit.request.messages)

        val compacted = recorder.assertSingleEffect<CommunicationSideEffect.MessagesCompacted>()
        assertEquals(commit, compacted.commit)
        assertEquals(listOf(alreadySent, trigger), compacted.originals)
    }

    @Test
    fun `compaction completed with multiple batches submits all commits and reports each batch`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val buffered = message(10, 3)
        val firstCommit = message(20, 4)
        val secondCommit = message(20, 5)

        var state = recordTransition(
            active(outgoingPendingRequest = request(alreadySent)),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()
        state = recordTransition(state, CommunicationStateEvent.SubmitMessage(buffered)).newState()

        val batches = listOf(
            CompactedBatch(firstCommit, listOf(alreadySent)),
            CompactedBatch(secondCommit, listOf(trigger))
        )
        val recorder = recordTransition(state, CommunicationStateEvent.CompactionCompleted(batches))

        val submit = recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>()
        assertEquals(listOf(firstCommit, secondCommit, buffered), submit.request.messages)

        val compacted = recorder.sideEffects.filterIsInstance<CommunicationSideEffect.MessagesCompacted>()
        assertEquals(listOf(firstCommit, secondCommit), compacted.map { it.commit })
        assertEquals(listOf(listOf(alreadySent), listOf(trigger)), compacted.map { it.originals })
    }

    @Test
    fun `overflowing compaction completed submits prefix and immediately compacts the leftover`() {
        val original = message(60, 1)
        val extraFirst = message(60, 2)
        val extraSecond = message(60, 3)
        val commit = message(20, 4)

        val state = active(
            pendingMessages = listOf(original, extraFirst, extraSecond),
            isCompacting = true
        )

        val recorder = recordTransition(
            state,
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(commit, listOf(original))))
        )

        assertEquals(listOf(commit, extraFirst), recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
        assertEquals(listOf(extraSecond), recorder.assertSingleEffect<CommunicationSideEffect.Compact>().messages)

        val buffered = recordTransition(recorder.newState(), CommunicationStateEvent.SubmitMessage(message(10, 5)))
        assertTrue(buffered.sideEffects.isEmpty())
    }

    @Test
    fun `second round compaction completed supersedes the in-flight prefix request without duplicates`() {
        val original = message(60, 1)
        val extraFirst = message(60, 2)
        val extraSecond = message(60, 3)
        val firstCommit = message(20, 4)
        val secondCommit = message(20, 5)

        val firstRoundRecorder = recordTransition(
            active(pendingMessages = listOf(original, extraFirst, extraSecond), isCompacting = true),
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(firstCommit, listOf(original))))
        )
        val prefixRequestId = firstRoundRecorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.requestId

        val recorder = recordTransition(
            firstRoundRecorder.newState(),
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(secondCommit, listOf(extraSecond))))
        )

        val submit = recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>()
        assertNotEquals(prefixRequestId, submit.request.requestId)
        assertEquals(listOf(firstCommit, extraFirst, secondCommit), submit.request.messages)
        recorder.assertNone<CommunicationSideEffect.Compact>()
    }

    @Test
    fun `commits-only remainder ends the compaction cycle and drains on response`() {
        val originalFirst = message(60, 1)
        val originalSecond = message(60, 2)
        val originalThird = message(60, 3)
        val firstCommit = message(60, 4)
        val secondCommit = message(60, 5)

        val state = active(
            pendingMessages = listOf(originalFirst, originalSecond, originalThird),
            isCompacting = true
        )

        val batches = listOf(
            CompactedBatch(firstCommit, listOf(originalFirst, originalSecond)),
            CompactedBatch(secondCommit, listOf(originalThird))
        )
        val recorder = recordTransition(state, CommunicationStateEvent.CompactionCompleted(batches))

        val submit = recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>()
        assertEquals(listOf(firstCommit), submit.request.messages)
        recorder.assertNone<CommunicationSideEffect.Compact>()

        val response = StatementTransportEvent.Response(requestId = submit.request.requestId, expiry = 11uL, responseCode = 0u)
        val responseRecorder = recordTransition(recorder.newState(), CommunicationStateEvent.ResponseReceived(response))
        assertEquals(listOf(secondCommit), responseRecorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
        responseRecorder.assertNone<CommunicationSideEffect.Compact>()
    }

    @Test
    fun `compaction completed while not compacting is ignored`() {
        val recorder = recordTransition(
            active(),
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(message(20, 4), listOf(message(10, 1)))))
        )

        assertTrue(recorder.sideEffects.isEmpty())
        assertTrue(recorder.states.isEmpty())
    }

    @Test
    fun `compaction failed while not compacting is ignored`() {
        val recorder = recordTransition(active(), CommunicationStateEvent.CompactionFailed)

        assertTrue(recorder.sideEffects.isEmpty())
        assertTrue(recorder.states.isEmpty())
    }

    @Test
    fun `compaction failed with oversized buffered message rejects it as too large`() {
        val oversized = message(150, 1)
        val compacting = recordTransition(active(), CommunicationStateEvent.SubmitMessage(oversized)).newState()

        val recorder = recordTransition(compacting, CommunicationStateEvent.CompactionFailed)

        recorder.assertSingleEffect<CommunicationSideEffect.NotifyMessageTooLarge>()
        recorder.assertNone<CommunicationSideEffect.SubmitRequest>()
    }

    @Test
    fun `compaction failed with no outstanding request drains sendable buffer immediately`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val buffered = message(10, 3)

        var state = recordTransition(
            active(outgoingPendingRequest = request(alreadySent, requestId = "outstanding")),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()
        state = recordTransition(state, CommunicationStateEvent.SubmitMessage(buffered)).newState()

        val response = StatementTransportEvent.Response(requestId = "outstanding", expiry = 11uL, responseCode = 0u)
        state = recordTransition(state, CommunicationStateEvent.ResponseReceived(response)).newState()

        val recorder = recordTransition(state, CommunicationStateEvent.CompactionFailed)

        assertEquals(listOf(trigger, buffered), recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
    }

    @Test
    fun `response received with overflowing buffer triggers re-compaction`() {
        val alreadySent = message(60, 1)
        val bufferedFirst = message(60, 2)
        val bufferedSecond = message(60, 3)

        val response = StatementTransportEvent.Response(requestId = "outstanding", expiry = 11uL, responseCode = 0u)
        val recorder = recordTransition(
            active(
                outgoingPendingRequest = request(alreadySent, requestId = "outstanding"),
                pendingMessages = listOf(bufferedFirst, bufferedSecond)
            ),
            CommunicationStateEvent.ResponseReceived(response)
        )

        assertEquals(listOf(bufferedFirst, bufferedSecond), recorder.assertSingleEffect<CommunicationSideEffect.Compact>().messages)
        recorder.assertNone<CommunicationSideEffect.SubmitRequest>()
    }

    @Test
    fun `compaction failed with outstanding request keeps it and drains on next response`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val outgoing = request(alreadySent, requestId = "outstanding")

        var state = recordTransition(
            active(outgoingPendingRequest = outgoing),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()

        val failedRecorder = recordTransition(state, CommunicationStateEvent.CompactionFailed)
        failedRecorder.assertNone<CommunicationSideEffect.SubmitRequest>()
        failedRecorder.assertNone<CommunicationSideEffect.NotifyMessageTooLarge>()
        state = failedRecorder.newState()

        val response = StatementTransportEvent.Response(requestId = "outstanding", expiry = 11uL, responseCode = 0u)
        val responseRecorder = recordTransition(state, CommunicationStateEvent.ResponseReceived(response))

        assertEquals(listOf(alreadySent), responseRecorder.assertSingleEffect<CommunicationSideEffect.ResponseReceived>().respondedMessages)
        assertEquals(listOf(trigger), responseRecorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
    }

    @Test
    fun `compaction failed with outstanding request appends fitting buffered messages to it`() {
        val alreadySent = message(60, 1)
        val small = message(10, 2)
        val large = message(60, 3)

        val state = active(
            outgoingPendingRequest = request(alreadySent, requestId = "outstanding"),
            pendingMessages = listOf(small, large),
            isCompacting = true
        )

        val recorder = recordTransition(state, CommunicationStateEvent.CompactionFailed)

        val submit = recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>()
        assertEquals(listOf(alreadySent, small), submit.request.messages)
        assertNotEquals("outstanding", submit.request.requestId)

        val response = StatementTransportEvent.Response(requestId = submit.request.requestId, expiry = 11uL, responseCode = 0u)
        val responseRecorder = recordTransition(recorder.newState(), CommunicationStateEvent.ResponseReceived(response))
        assertEquals(listOf(large), responseRecorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
    }

    @Test
    fun `lone oversized message with canCompact is compacted instead of rejected`() {
        val oversized = message(150, 1)

        val recorder = recordTransition(active(), CommunicationStateEvent.SubmitMessage(oversized))

        assertEquals(listOf(oversized), recorder.assertSingleEffect<CommunicationSideEffect.Compact>().messages)
        recorder.assertNone<CommunicationSideEffect.NotifyMessageTooLarge>()
    }

    @Test
    fun `lone oversized message without canCompact is rejected as too large`() {
        val oversized = message(150, 1)

        val recorder = recordTransition(active(canCompact = false), CommunicationStateEvent.SubmitMessage(oversized))

        recorder.assertSingleEffect<CommunicationSideEffect.NotifyMessageTooLarge>()
        recorder.assertNone<CommunicationSideEffect.Compact>()
        assertTrue(recorder.states.isEmpty())
    }

    @Test
    fun `response for a superseded request id is ignored after compaction`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val commit = message(20, 3)
        val supersededId = "superseded"

        var state = recordTransition(
            active(outgoingPendingRequest = request(alreadySent, requestId = supersededId)),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()
        val completedRecorder = recordTransition(
            state,
            CommunicationStateEvent.CompactionCompleted(listOf(CompactedBatch(commit, listOf(alreadySent, trigger))))
        )
        val newRequestId = completedRecorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.requestId
        assertNotEquals(supersededId, newRequestId)
        state = completedRecorder.newState()

        val staleResponse = StatementTransportEvent.Response(requestId = supersededId, expiry = 12uL, responseCode = 0u)
        val recorder = recordTransition(state, CommunicationStateEvent.ResponseReceived(staleResponse))

        assertTrue(recorder.sideEffects.isEmpty())
        assertTrue(recorder.states.isEmpty())
    }

    @Test
    fun `response received during compaction does not drain buffered messages`() {
        val alreadySent = message(60, 1)
        val trigger = message(60, 2)
        val outgoing = request(alreadySent, requestId = "outstanding")

        val compacting = recordTransition(
            active(outgoingPendingRequest = outgoing),
            CommunicationStateEvent.SubmitMessage(trigger)
        ).newState()

        val response = StatementTransportEvent.Response(requestId = "outstanding", expiry = 11uL, responseCode = 0u)
        val recorder = recordTransition(compacting, CommunicationStateEvent.ResponseReceived(response))

        recorder.assertSingleEffect<CommunicationSideEffect.ResponseReceived>()
        recorder.assertNone<CommunicationSideEffect.SubmitRequest>()
    }

    @Test
    fun `without canCompact growing append submits combined request as before`() {
        val alreadySent = message(60, 1)
        val newMessage = message(30, 2)

        val recorder = recordTransition(
            active(outgoingPendingRequest = request(alreadySent), canCompact = false),
            CommunicationStateEvent.SubmitMessage(newMessage)
        )

        assertEquals(listOf(alreadySent, newMessage), recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
        recorder.assertNone<CommunicationSideEffect.Compact>()
    }

    @Test
    fun `without canCompact overflowing append buffers and drains on response as before`() {
        val alreadySent = message(60, 1)
        val newMessage = message(60, 2)
        val outgoing = request(alreadySent, requestId = "outstanding")

        val bufferRecorder = recordTransition(
            active(outgoingPendingRequest = outgoing, canCompact = false),
            CommunicationStateEvent.SubmitMessage(newMessage)
        )
        assertTrue(bufferRecorder.sideEffects.isEmpty())

        val response = StatementTransportEvent.Response(requestId = "outstanding", expiry = 11uL, responseCode = 0u)
        val recorder = recordTransition(bufferRecorder.newState(), CommunicationStateEvent.ResponseReceived(response))

        assertEquals(listOf(newMessage), recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
        recorder.assertNone<CommunicationSideEffect.Compact>()
    }

    @Test
    fun `incoming request is auto-acked exactly once`() {
        val incoming = request(message(10, 1), requestId = "incoming")

        val recorder = recordTransition(active(), CommunicationStateEvent.RequestReceived(incoming))

        recorder.assertSingleEffect<CommunicationSideEffect.RequestReceived>()
        val ack = recorder.assertSingleEffect<CommunicationSideEffect.SubmitResponse>()
        assertEquals("incoming", ack.response.requestId)
        assertEquals(0u.toUByte(), ack.response.responseCode)

        val redelivered = recordTransition(recorder.newState(), CommunicationStateEvent.RequestReceived(incoming))
        assertTrue(redelivered.sideEffects.isEmpty())
    }

    @Test
    fun `initialization auto-acks restored unresponded requests`() {
        val unresponded = request(message(10, 1), requestId = "unresponded")
        val initialization = Initialization(emptyList(), MAX_STATEMENT_SIZE, canCompact = true)

        val recorder = recordTransition(
            initialization,
            CommunicationStateEvent.InitialDataFetched(
                outgoingPendingRequest = null,
                incomingRequests = mapOf(unresponded to NOT_RESPONDED),
                lastUsedExpiry = 10uL
            )
        )

        recorder.assertSingleEffect<CommunicationSideEffect.RequestReceived>()
        assertEquals("unresponded", recorder.assertSingleEffect<CommunicationSideEffect.SubmitResponse>().response.requestId)

        val redelivered = recordTransition(recorder.newState(), CommunicationStateEvent.RequestReceived(unresponded))
        assertTrue(redelivered.sideEffects.isEmpty())
    }

    @Test
    fun `initialization with oversized backlog and canCompact compacts on activation`() {
        val backlog = listOf(message(60, 1), message(60, 2))
        val initialization = Initialization(backlog, MAX_STATEMENT_SIZE, canCompact = true)

        val recorder = recordTransition(
            initialization,
            CommunicationStateEvent.InitialDataFetched(
                outgoingPendingRequest = null,
                incomingRequests = emptyMap(),
                lastUsedExpiry = 10uL
            )
        )

        assertEquals(backlog, recorder.assertSingleEffect<CommunicationSideEffect.Compact>().messages)
        recorder.assertNone<CommunicationSideEffect.SubmitRequest>()
    }

    @Test
    fun `initialization without canCompact packs backlog partially as before`() {
        val backlog = listOf(message(60, 1), message(60, 2))
        val initialization = Initialization(backlog, MAX_STATEMENT_SIZE, canCompact = false)

        val recorder = recordTransition(
            initialization,
            CommunicationStateEvent.InitialDataFetched(
                outgoingPendingRequest = null,
                incomingRequests = emptyMap(),
                lastUsedExpiry = 10uL
            )
        )

        assertEquals(listOf(backlog.first()), recorder.assertSingleEffect<CommunicationSideEffect.SubmitRequest>().request.messages)
        recorder.assertNone<CommunicationSideEffect.Compact>()
    }
}
