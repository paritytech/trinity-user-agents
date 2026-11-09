package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId

class HostRpcConnectedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> {
        every { nowMillis() } returnsMany listOf(DIALLED_AT, FIRST_FRAME_AT)
    }
    private val captured = mutableListOf<HostEvent>()
    private val telemetry = HostRpcConnectedTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() })

    @Test
    fun `the first frame on a connection reports the chain and how long it took to answer`() {
        telemetry.observer({ null }) { "people" }.dialling(CONNECTION, GENESIS, attempt = 0)

        telemetry.responded(CONNECTION)

        assertEquals(
            HostEvent(
                name = "host.rpc.connected",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "host.os_version" to "15",
                    "rpc.chain" to "people",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to FIRST_FRAME_AT,
                    "duration_ms" to FIRST_FRAME_AT - DIALLED_AT,
                    "app.launch_id" to "launch-7",
                    "failover_count" to 0,
                ),
                durationMs = FIRST_FRAME_AT - DIALLED_AT,
            ),
            captured.single(),
        )
    }

    // A dead node is replaced under the same connection, so the core sees one slow dial: the count says why.
    @Test
    fun `each endpoint tried after a dead one counts as a failover, timed from the first dial`() {
        val observer = telemetry.observer({ null }) { "people" }
        observer.dialling(CONNECTION, GENESIS, attempt = 0)
        observer.dialling(CONNECTION, GENESIS, attempt = 1)
        observer.dialling(CONNECTION, GENESIS, attempt = 2)

        telemetry.responded(CONNECTION)

        val event = captured.single()
        assertEquals(2, event.extras["failover_count"])
        assertEquals(FIRST_FRAME_AT - DIALLED_AT, event.durationMs)
    }

    // The page and the worker of one product dial the same chains; only these tell their connects apart.
    @Test
    fun `a connection dialled for a product execution names the product, its kind and its session`() {
        val executable = HostRpcExecutable(HUMANITY, ExecutableKind.WORKER, "worker-session")
        telemetry.observer({ executable }) { "people" }.dialling(CONNECTION, GENESIS, attempt = 0)

        telemetry.responded(CONNECTION)

        val tags = captured.single().tags
        assertEquals(
            listOf("humanity.dot", "worker", "worker-session"),
            listOf(tags["product.id"], tags["executable.kind"], tags["host.session_id"]),
        )
    }

    @Test
    fun `later frames on the same connection report nothing more`() {
        telemetry.observer({ null }) { "people" }.dialling(CONNECTION, GENESIS, attempt = 0)

        repeat(3) { telemetry.responded(CONNECTION) }

        assertEquals(1, captured.size)
    }

    @Test
    fun `a connection that closes before any frame is reported as an error`() {
        telemetry.observer({ null }) { "bulletin" }.dialling(CONNECTION, GENESIS, attempt = 0)

        telemetry.closedBeforeResponse(CONNECTION)
        telemetry.responded(CONNECTION)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    // Backgrounding or a network change closes every connection; none of them failed.
    @Test
    fun `a connection the host closes before any frame is reported as abandoned, not as an error`() {
        val observer = telemetry.observer({ null }) { "people" }
        observer.dialling(CONNECTION, GENESIS, attempt = 0)

        observer.closingByHost(CONNECTION)
        telemetry.closedBeforeResponse(CONNECTION)

        assertEquals(listOf("abandoned"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a connection the core closes itself reports nothing`() {
        telemetry.observer({ null }) { "people" }.dialling(CONNECTION, GENESIS, attempt = 0)

        telemetry.abandoned(CONNECTION)
        telemetry.closedBeforeResponse(CONNECTION)

        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `nothing is kept for a dial while host telemetry is off`() {
        val off = HostRpcConnectedTelemetry(
            originProvider,
            timeProvider,
            object : HostEventCapture {
                override val enabled = false

                override fun capture(event: () -> HostEvent) = error("no capture while off")
            },
        )

        off.observer({ error("no executable read while off") }) { "people" }.dialling(CONNECTION, GENESIS, attempt = 0)
        off.responded(CONNECTION)

        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    private companion object {
        val CONNECTION = 7u
        val GENESIS = byteArrayOf(1, 2, 3)
        val HUMANITY = ProductId.parse("humanity.dot").getOrThrow()
        const val DIALLED_AT = 2_000L
        const val FIRST_FRAME_AT = 2_350L
        val ORIGIN = HostEventOrigin(
            appVariant = "nightly",
            appFlavor = "gp",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "15",
            appBuild = 123L,
            launchId = "launch-7",
        )
    }
}
