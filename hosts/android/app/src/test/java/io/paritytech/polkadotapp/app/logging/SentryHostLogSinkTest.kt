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

class SentryHostLogSinkTest {
    private val logger = mockk<ILoggerApi>(relaxed = true)
    private val sink = SentryHostLogSink(logger)

    @Test
    fun `a line reaches Sentry with its attributes beside its tag`() {
        sink.log(
            Log.WARN,
            "truapi.core",
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

    @Test
    fun `a line carrying no attributes still carries its tag`() {
        sink.log(Log.INFO, "truapi.chain", "chain connection 1 dialling", emptyMap())

        val parameters = slot<SentryLogParameters>()
        verify(exactly = 1) {
            logger.log(SentryLogLevel.INFO, capture(parameters), "chain connection 1 dialling")
        }
        assertEquals("truapi.chain", parameters.captured.attributes?.attributes?.get("tag")?.value)
    }

    @Test
    fun `warnings and errors keep their level`() {
        sink.log(Log.WARN, "truapi.chain", "closing", emptyMap())
        sink.log(Log.ERROR, "truapi.chain", "failed", emptyMap())

        verify(exactly = 1) {
            logger.log(SentryLogLevel.WARN, any<SentryLogParameters>(), "closing")
            logger.log(SentryLogLevel.ERROR, any<SentryLogParameters>(), "failed")
        }
    }

    @Test
    fun `an assert lands at error, as on iOS`() {
        sink.log(Log.ASSERT, "truapi.chain", "lost", emptyMap())

        verify(exactly = 1) {
            logger.log(SentryLogLevel.ERROR, any<SentryLogParameters>(), "lost")
        }
    }
}
