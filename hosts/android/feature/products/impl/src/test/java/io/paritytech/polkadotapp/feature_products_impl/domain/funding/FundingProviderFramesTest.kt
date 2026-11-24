package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.ProductId

class FundingProviderFramesTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val intent = FundingIntentId("intent")

    // What the core waits on outlives each test, so the test does not wait for it.
    private val detached = Job()
    private val core = FakeFundingCore()
    private val frames = RealFundingProviderFrames(core)

    @Test
    fun `a provider asking for a screen is told to the app, which opens it in the core`() = runTest {
        core.waitingFrames += "intent"

        frames.requested(ramp, intent)

        assertEquals(FundingFrameEvent.Requested(intent, ramp), frames.events.first())
        assertTrue(frames.isWaiting(intent).getOrThrow())
        assertTrue(frames.open(intent).getOrThrow())
        assertFalse(frames.isWaiting(intent).getOrThrow())
        assertFalse(frames.open(intent).getOrThrow())
    }

    @Test
    fun `a provider's screen the core presents is answered as the app took it down`() = runTest {
        val target = FundingFrameTarget.App("/kyc")
        val outcome = async(detached) { frames.present(ramp, intent, target) }
        val request = (frames.events.first() as FundingFrameEvent.Opened).request

        frames.answer(request, FundingFrameOutcome.Dismissed)

        assertEquals(target, request.target)
        assertEquals(FundingFrameOutcome.Dismissed, outcome.await())
    }

    @Test
    fun `the provider closing its screens answers them closed and takes them down`() = runTest {
        val outcome = async(detached) { frames.present(ramp, intent, FundingFrameTarget.App("/kyc")) }
        frames.events.first()

        frames.close(ramp, intent)

        assertEquals(FundingFrameOutcome.Closed, outcome.await())
        assertEquals(FundingFrameEvent.Closed(intent), frames.events.first())
    }
}
