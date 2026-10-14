package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.novasama.substrate_sdk_android.wsrpc.exception.RpcException
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import kotlin.time.Duration
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class HopTransferRetryPolicyTest {
    private val policy = HopTransferRetryPolicy()

    private val now = Instant.fromEpochMilliseconds(1_700_000_000_000L)

    @Test
    fun `backoff doubles from 10s and caps at 1h`() {
        assertEquals(10.seconds, rescheduleDelayForAttempt(1))
        assertEquals(20.seconds, rescheduleDelayForAttempt(2))
        assertEquals(40.seconds, rescheduleDelayForAttempt(3))
        assertEquals(80.seconds, rescheduleDelayForAttempt(4))
        assertEquals(1.hours, rescheduleDelayForAttempt(100))
    }

    @Test
    fun `first recoverable failure seeds the window and schedules 10s later`() {
        val decision = policy.decide(HopTransferRetryState.None, IOException(), now)

        assertTrue(decision is RetryDecision.Reschedule)
        decision as RetryDecision.Reschedule
        assertEquals(1, decision.attemptCount)
        assertEquals(now, decision.firstFailureAt)
        assertEquals(now + 10.seconds, decision.nextAttemptAt)
    }

    @Test
    fun `subsequent recoverable failure preserves the window anchor and grows backoff`() {
        val firstFailureAt = now - 1.hours

        val decision = policy.decide(
            HopTransferRetryState(attemptCount = 3, firstFailureAt = firstFailureAt, nextAttemptAt = null),
            RetryableTransferException("still promoting"),
            now
        )

        assertTrue(decision is RetryDecision.Reschedule)
        decision as RetryDecision.Reschedule
        assertEquals(4, decision.attemptCount)
        assertEquals(firstFailureAt, decision.firstFailureAt)
        assertEquals(now + 80.seconds, decision.nextAttemptAt)
    }

    @Test
    fun `recoverable failure past the 24h window is terminal`() {
        val decision = policy.decide(
            HopTransferRetryState(attemptCount = 50, firstFailureAt = now - 24.hours - 1.milliseconds, nextAttemptAt = null),
            IOException(),
            now
        )

        assertEquals(RetryDecision.Terminal, decision)
    }

    @Test
    fun `only explicitly terminal errors are terminal immediately`() {
        assertEquals(RetryDecision.Terminal, policy.decide(HopTransferRetryState.None, TerminalTransferException("bad cid"), now))
    }

    @Test
    fun `any unclassified error is recoverable`() {
        assertTrue(policy.decide(HopTransferRetryState.None, IOException(), now) is RetryDecision.Reschedule)
        assertTrue(policy.decide(HopTransferRetryState.None, RpcException("busy"), now) is RetryDecision.Reschedule)
        assertTrue(policy.decide(HopTransferRetryState.None, RetryableTransferException("lag"), now) is RetryDecision.Reschedule)
        assertTrue(policy.decide(HopTransferRetryState.None, IllegalStateException("decode"), now) is RetryDecision.Reschedule)
    }

    @Test
    fun `cancellation is ignored without consuming the retry window`() {
        assertEquals(RetryDecision.Ignore, policy.decide(HopTransferRetryState.None, TransferCancelledException(), now))
    }

    private fun rescheduleDelayForAttempt(attemptCount: Int): Duration {
        val current = HopTransferRetryState(attemptCount = attemptCount - 1, firstFailureAt = now, nextAttemptAt = null)
        val decision = policy.decide(current, IOException(), now) as RetryDecision.Reschedule
        return decision.nextAttemptAt - now
    }
}
