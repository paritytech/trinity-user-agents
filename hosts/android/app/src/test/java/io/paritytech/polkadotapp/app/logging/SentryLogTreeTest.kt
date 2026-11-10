package io.paritytech.polkadotapp.app.logging

import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import io.sentry.SentryLogLevel
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class SentryLogTreeTest {
    private val logger = mockk<ILoggerApi>(relaxed = true)

    @Before
    fun plant() {
        Timber.plant(SentryLogTree(logger))
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    @Test
    fun `a truapi line reaches Sentry with its tag`() {
        Timber.tag("truapi.chain").i("chain connection %d dialling %s", 1, "wss://node.example")

        val parameters = slot<SentryLogParameters>()
        verify(exactly = 1) {
            logger.log(SentryLogLevel.INFO, capture(parameters), "chain connection 1 dialling wss://node.example")
        }
        assertEquals("truapi.chain", parameters.captured.attributes?.attributes?.get("tag")?.value)
    }

    @Test
    fun `warnings and errors keep their level`() {
        Timber.tag("truapi.chain").w("chain connection 1 closing")
        Timber.tag("truapi.chain").e("chain connection 1 failed")

        verify(exactly = 1) {
            logger.log(SentryLogLevel.WARN, any<SentryLogParameters>(), "chain connection 1 closing")
            logger.log(SentryLogLevel.ERROR, any<SentryLogParameters>(), "chain connection 1 failed")
        }
    }

    @Test
    fun `other tags and debug lines stay on the device`() {
        Timber.tag("BearerTokenAuthenticator").w("refresh failed")
        Timber.tag("truapi.core").d("detail")

        verify(exactly = 0) { logger.log(any<SentryLogLevel>(), any<SentryLogParameters>(), any<String>()) }
    }
}
