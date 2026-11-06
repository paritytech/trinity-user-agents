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
import org.junit.Assert.assertFalse
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
        telemetry.dialled(CONNECTION, executable = null) { "people" }

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
                    "chain" to "people",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to FIRST_FRAME_AT,
                    "duration_ms" to FIRST_FRAME_AT - DIALLED_AT,
                ),
                durationMs = FIRST_FRAME_AT - DIALLED_AT,
            ),
            captured.single(),
        )
    }

    // The page and the worker of one product dial the same chains; only these tell their connects apart.
    @Test
    fun `a connection dialled for a product execution names the product, its kind and its session`() {
        telemetry.dialled(CONNECTION, HostRpcExecutable(HUMANITY, ExecutableKind.WORKER, "worker-session")) { "people" }

        telemetry.responded(CONNECTION)

        val tags = captured.single().tags
        assertEquals(
            listOf("humanity.dot", "worker", "worker-session"),
            listOf(tags["product.id"], tags["executable.kind"], tags["host.session_id"]),
        )
    }

    @Test
    fun `an app or widget execution is a page`() {
        telemetry.dialled(CONNECTION, HostRpcExecutable(HUMANITY, ExecutableKind.WIDGET, "page-session")) { "people" }

        telemetry.responded(CONNECTION)

        assertEquals("page", captured.single().tags["executable.kind"])
    }

    @Test
    fun `later frames on the same connection report nothing more`() {
        telemetry.dialled(CONNECTION, executable = null) { "people" }

        repeat(3) { telemetry.responded(CONNECTION) }

        assertEquals(1, captured.size)
    }

    @Test
    fun `a connection that closes before any frame is reported as an error`() {
        telemetry.dialled(CONNECTION, executable = null) { "bulletin" }

        telemetry.closedBeforeResponse(CONNECTION)
        telemetry.responded(CONNECTION)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a connection the core closes itself reports nothing`() {
        telemetry.dialled(CONNECTION, executable = null) { "people" }

        telemetry.abandoned(CONNECTION)
        telemetry.closedBeforeResponse(CONNECTION)

        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `the event and chain label are not built when capture drops it`() {
        var chainLabelRead = false
        val dropping = HostRpcConnectedTelemetry(originProvider, timeProvider, HostEventCapture { })
        dropping.dialled(CONNECTION, executable = null) {
            chainLabelRead = true
            "people"
        }

        dropping.responded(CONNECTION)

        assertFalse(chainLabelRead)
        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 1) { timeProvider.nowMillis() }
    }

    private companion object {
        val CONNECTION = 7u
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
        )
    }
}
