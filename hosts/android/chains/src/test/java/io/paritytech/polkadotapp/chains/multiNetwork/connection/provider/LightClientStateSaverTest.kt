package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class LightClientStateSaverTest {
    private val lifecycle = MutableStateFlow(AppLifecycleState.FOREGROUND)
    private val storage = mockk<LightClientStorage>(relaxed = true)
    private val savedAt = mutableListOf<Long>()

    // A snapshot is a multi-megabyte write, so saving must back off rather than repeat every minute.
    @Test
    fun `saves 30 s after start, then at an interval doubling up to 10 min`() = runTest {
        start(lightClientOn = true)

        advanceTimeBy(30.minutes)
        runCurrent()

        assertEquals(listOf("0:30", "1:30", "3:30", "7:30", "15:30", "25:30"), savedAt.map(::minutesAndSeconds))
    }

    // In the background the chains are stopped; a save then has nothing to read and wakes the app for
    // nothing. Coming back starts over, since smoldot resumed from what was stored.
    @Test
    fun `stops saving in the background and starts over on return`() = runTest {
        start(lightClientOn = true)
        advanceTimeBy(1.minutes)
        lifecycle.value = AppLifecycleState.BACKGROUND
        advanceTimeBy(10.minutes)
        lifecycle.value = AppLifecycleState.FOREGROUND

        advanceTimeBy(31.seconds)
        runCurrent()

        assertEquals(listOf("0:30", "11:30"), savedAt.map(::minutesAndSeconds))
    }

    // An earlier run with the light client on left its state behind; nothing reads it while off.
    @Test
    fun `with the light client off it clears stored state and never saves`() = runTest {
        start(lightClientOn = false)

        advanceTimeBy(30.minutes)
        runCurrent()

        coVerify(exactly = 1) { storage.clear() }
        assertEquals(emptyList<Long>(), savedAt)
    }

    @Test
    fun `with the light client on it keeps stored state`() = runTest {
        start(lightClientOn = true)

        advanceTimeBy(30.minutes)
        runCurrent()

        coVerify(exactly = 0) { storage.clear() }
    }

    private fun TestScope.start(lightClientOn: Boolean) {
        val lightClient = mockk<LightClient> {
            every { servesAny() } returns lightClientOn
            coEvery { saveDatabases() } answers { savedAt += currentTime }
        }
        val observer = mockk<AppLifecycleObserver> { every { subscribe() } returns lifecycle }
        val saver = LightClientStateSaver(lightClient, storage, observer, testDispatchers())

        with(ComputationalScope(backgroundScope)) { saver.initialize().getOrThrow() }
    }

    private fun minutesAndSeconds(millis: Long): String {
        val seconds = millis / 1000
        return "${seconds / 60}:${"%02d".format(seconds % 60)}"
    }
}
