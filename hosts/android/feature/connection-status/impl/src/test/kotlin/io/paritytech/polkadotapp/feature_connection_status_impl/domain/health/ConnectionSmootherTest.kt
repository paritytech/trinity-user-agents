package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime

@OptIn(ExperimentalTime::class, ExperimentalCoroutinesApi::class)
class ConnectionSmootherTest {
    private val config = ConnectionSmoothingConfig(
        lossHold = 3.seconds,
        flapWindow = 30.seconds,
        flapDropThreshold = 2,
        flapHold = 10.seconds,
        sustainedDisconnect = 5.seconds,
    )

    @Test
    fun `a first connect reports connected immediately`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        assertEquals(listOf(ChainConnectionPresentation.Connected), results)
    }

    @Test
    fun `a first pending reports connecting immediately`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Pending); runCurrent()

        assertEquals(listOf(ChainConnectionPresentation.Connecting), results)
    }

    @Test
    fun `a reconnect reports connected immediately`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()
        source.emit(RawConnectivity.Pending)
        advanceTimeBy(3_500); runCurrent()
        assertEquals(ChainConnectionPresentation.Connecting, results.last())

        source.emit(RawConnectivity.Connected); runCurrent()

        assertEquals(ChainConnectionPresentation.Connected, results.last())
    }

    @Test
    fun `a drop reports connecting only after the loss hold`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        source.emit(RawConnectivity.Pending)
        advanceTimeBy(1_000); runCurrent()
        assertEquals(ChainConnectionPresentation.Connected, results.last())

        advanceTimeBy(2_500); runCurrent() // total 3.5s > 3s
        assertEquals(ChainConnectionPresentation.Connecting, results.last())
    }

    @Test
    fun `a drop that reconnects inside the loss hold never reports connecting`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        source.emit(RawConnectivity.Pending)
        advanceTimeBy(1_000); runCurrent()
        source.emit(RawConnectivity.Connected)
        advanceTimeBy(5_000); runCurrent()

        assertEquals(listOf(ChainConnectionPresentation.Connected), results)
    }

    @Test
    fun `a reconnect after the network came back reports connecting at once`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()
        source.emit(RawConnectivity.Offline); runCurrent()
        assertEquals(ChainConnectionPresentation.Offline, results.last())

        source.emit(RawConnectivity.Pending); runCurrent()

        assertEquals(ChainConnectionPresentation.Connecting, results.last())
    }

    @Test
    fun `flapping extends the loss hold`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        // first drop: only one drop in the window, so the plain loss hold applies
        source.emit(RawConnectivity.Pending)
        advanceTimeBy(3_500); runCurrent()
        assertEquals(ChainConnectionPresentation.Connecting, results.last())
        source.emit(RawConnectivity.Connected); runCurrent()

        // second drop within the window -> connecting waits out the longer flap hold
        source.emit(RawConnectivity.Pending)
        advanceTimeBy(4_000); runCurrent() // past 3s loss hold, before 10s flap hold
        assertEquals(ChainConnectionPresentation.Connected, results.last())

        advanceTimeBy(7_000); runCurrent() // now past the flap hold
        assertEquals(ChainConnectionPresentation.Connecting, results.last())
    }

    @Test
    fun `a drop long after earlier drops aged out uses the plain loss hold`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()
        source.emit(RawConnectivity.Pending)
        advanceTimeBy(3_500); runCurrent()
        source.emit(RawConnectivity.Connected)
        advanceTimeBy(31_000); runCurrent() // past the 30s flap window, so the first drop is purged

        source.emit(RawConnectivity.Pending)
        advanceTimeBy(3_500); runCurrent()

        assertEquals(ChainConnectionPresentation.Connecting, results.last())
    }

    @Test
    fun `a settled disconnect reports disconnected only after the cooldown`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        source.emit(RawConnectivity.Settled); runCurrent()
        assertEquals(ChainConnectionPresentation.Connecting, results.last())

        advanceTimeBy(5_500); runCurrent()
        assertEquals(ChainConnectionPresentation.Disconnected, results.last())
    }

    @Test
    fun `a settled disconnect repeated faster than the cooldown never reports disconnected`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        repeat(10) {
            source.emit(RawConnectivity.Settled); runCurrent()
            advanceTimeBy(1_000); runCurrent()
        }

        assertEquals(ChainConnectionPresentation.Connecting, results.last())
    }

    @Test
    fun `a device with no internet is reported at once rather than waiting out the cooldown`() = runTest {
        val results = collectSmoothed()

        source.emit(RawConnectivity.Connected); runCurrent()

        source.emit(RawConnectivity.Offline); runCurrent()

        assertEquals(ChainConnectionPresentation.Offline, results.last())
    }

    private lateinit var source: MutableSharedFlow<RawConnectivity>

    private fun TestScope.collectSmoothed(): List<ChainConnectionPresentation> {
        source = MutableSharedFlow(extraBufferCapacity = 64)
        val smoother = ConnectionSmoother(config, FakeTimeProvider { testScheduler.currentTime })
        val results = mutableListOf<ChainConnectionPresentation>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            smoother.smooth(source).collect { results += it }
        }
        return results
    }
}
