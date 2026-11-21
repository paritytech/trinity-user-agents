package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import io.sentry.SentryLogLevel
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import org.junit.Assert.assertEquals
import org.junit.Test

class SentryCoreEventSinkTest {
    private val logger = mockk<ILoggerApi>(relaxed = true)

    @Test
    fun `a core event reaches Sentry with its attributes beside its tag`() {
        SentryCoreEventSink(logger).log(
            Log.WARN,
            "truapi.chain.follow_budget_reached",
            mapOf("outcome" to "sent", "age_ms" to 1200L, "money_moved" to true),
        )

        val parameters = slot<SentryLogParameters>()
        verify(exactly = 1) {
            logger.log(SentryLogLevel.WARN, capture(parameters), "truapi.chain.follow_budget_reached")
        }
        val attributes = parameters.captured.attributes!!.attributes
        assertEquals("truapi.core", attributes["tag"]?.value)
        assertEquals("sent", attributes["outcome"]?.value)
        assertEquals(1200L, attributes["age_ms"]?.value)
        assertEquals(true, attributes["money_moved"]?.value)
    }
}
