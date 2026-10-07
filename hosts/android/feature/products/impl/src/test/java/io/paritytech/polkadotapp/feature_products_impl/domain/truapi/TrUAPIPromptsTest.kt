package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIPromptsTest {
    /** Records what the prompt asked the navigation to do. */
    private class RecordingPrompts : TrUAPIPrompts<String, String>(unanswered = "dismissed") {
        val events = mutableListOf<String>()
        override suspend fun open() {
            events += "open"
        }
        override suspend fun close() {
            events += "close"
        }
    }

    private fun RecordingPrompts.shown() = checkNotNull(current).also { it.markShown() }

    @Test
    fun `the screen's first answer is the one the core gets`() = runTest {
        val prompts = RecordingPrompts()
        val asked = async(start = CoroutineStart.UNDISPATCHED) { prompts.ask("question") }
        val prompt = prompts.shown()
        prompt.answer("picked")
        prompt.answer("too late")

        assertEquals("picked", asked.await())
        assertEquals(listOf("open"), prompts.events)
    }

    @Test
    fun `a screen that never appears answers unanswered instead of holding the core`() = runTest {
        val prompts = RecordingPrompts()

        assertEquals("dismissed", prompts.ask("question"))
    }

    @Test
    fun `a cancelled prompt closes its screen and lets the next one open`() = runTest {
        // The product withdrew the call, so nothing may stay on screen for it.
        val prompts = RecordingPrompts()
        val asked = async(start = CoroutineStart.UNDISPATCHED) { prompts.ask("first") }
        prompts.shown()
        asked.cancel()
        advanceUntilIdle()

        val next = async(start = CoroutineStart.UNDISPATCHED) { prompts.ask("second") }
        prompts.shown().answer("picked")

        assertEquals("picked", next.await())
        assertEquals(listOf("open", "close", "open"), prompts.events)
        assertTrue(asked.isCancelled)
    }
}
