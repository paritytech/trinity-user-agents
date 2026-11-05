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

class HostRpcConnectedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> {
        every { nowMillis() } returnsMany listOf(DIALLED_AT, FIRST_FRAME_AT)
    }
    private val captured = mutableListOf<HostEvent>()
    private val telemetry = HostRpcConnectedTelemetry(originProvider, timeProvider, HostEventCapture { captured += it() })

    @Test
    fun `the first frame on a connection reports the chain and how long it took to answer`() {
        telemetry.dialled(CONNECTION) { "people" }

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
            ),
            captured.single(),
        )
    }

    @Test
    fun `later frames on the same connection report nothing more`() {
        telemetry.dialled(CONNECTION) { "people" }

        repeat(3) { telemetry.responded(CONNECTION) }

        assertEquals(1, captured.size)
    }

    @Test
    fun `a connection that closes before any frame is reported as an error`() {
        telemetry.dialled(CONNECTION) { "bulletin" }

        telemetry.closedBeforeResponse(CONNECTION)
        telemetry.responded(CONNECTION)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `a connection the core closes itself reports nothing`() {
        telemetry.dialled(CONNECTION) { "people" }

        telemetry.abandoned(CONNECTION)
        telemetry.closedBeforeResponse(CONNECTION)

        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `the event and chain label are not built when capture drops it`() {
        var chainLabelRead = false
        val dropping = HostRpcConnectedTelemetry(originProvider, timeProvider, HostEventCapture { })
        dropping.dialled(CONNECTION) {
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
