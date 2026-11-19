package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.util.Log
import io.mockk.mockk
import io.mockk.spyk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.CoreEventSink
import org.junit.After
import org.junit.Before
import org.junit.Test
import timber.log.Timber
import uniffi.truapi.CoreAttribute
import uniffi.truapi.CoreAttributeValue

class CoreEventLoggerTest {
    private val tree = spyk(
        object : Timber.Tree() {
            public override fun log(priority: Int, tag: String?, message: String, t: Throwable?) = Unit
        },
    )
    private val sink = mockk<CoreEventSink>(relaxed = true)
    private val logger = CoreEventLogger(sink)
    private val attributes = listOf(
        CoreAttribute("outcome", CoreAttributeValue.Text("sent")),
        CoreAttribute("age_ms", CoreAttributeValue.Count(1200uL)),
        CoreAttribute("money_moved", CoreAttributeValue.Flag(true)),
    )

    @Before
    fun plant() {
        Timber.plant(tree)
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    @Test
    fun `an exported marker reaches the sink with its attributes`() {
        logger.log("truapi.chain.follow_budget_reached", attributes)

        verify(exactly = 1) {
            sink.log(
                Log.WARN,
                "truapi.chain.follow_budget_reached",
                mapOf("outcome" to "sent", "age_ms" to 1200L, "money_moved" to true),
            )
        }
        verify(exactly = 1) {
            tree.log(
                Log.DEBUG,
                "truapi.core",
                "truapi.chain.follow_budget_reached: outcome=sent age_ms=1200 money_moved=true",
                null,
            )
        }
    }

    @Test
    fun `a local marker stays on the device`() {
        logger.log("truapi.native.callback.push_notification", attributes)

        verify(exactly = 0) { sink.log(any(), any(), any()) }
    }

    @Test
    fun `a marker the table does not list stays on the device`() {
        logger.log("truapi.chain.not_registered", attributes)

        verify(exactly = 0) { sink.log(any(), any(), any()) }
    }
}
