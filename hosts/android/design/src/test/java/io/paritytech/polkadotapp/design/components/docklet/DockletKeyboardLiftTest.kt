package io.paritytech.polkadotapp.design.components.docklet

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class DockletKeyboardLiftTest {
    @Test
    fun `dock is not lifted when the keyboard is already up at entry`() = runTest {
        val lift = startLift(cardOpen = false, keyboardUp = true)

        assertTrue(lift.changes.isEmpty())
    }

    @Test
    fun `dock is lifted when the card closes with the keyboard up`() = runTest {
        val lift = startLift(cardOpen = true, keyboardUp = true)

        lift.cardOpen.value = false
        runCurrent()

        assertEquals(listOf(true), lift.changes)
    }

    @Test
    fun `dock is not lifted when the card closes with the keyboard down`() = runTest {
        val lift = startLift(cardOpen = true, keyboardUp = false)

        lift.cardOpen.value = false
        runCurrent()

        assertTrue(lift.changes.isEmpty())
    }

    @Test
    fun `lift holds when the card reopens while the keyboard is leaving`() = runTest {
        val lift = startLift(cardOpen = true, keyboardUp = true)
        lift.cardOpen.value = false
        runCurrent()

        lift.cardOpen.value = true
        runCurrent()

        assertEquals(listOf(true), lift.changes)
    }

    @Test
    fun `lift releases when the keyboard is gone`() = runTest {
        val lift = startLift(cardOpen = true, keyboardUp = true)
        lift.cardOpen.value = false
        runCurrent()

        lift.keyboardUp.value = false
        runCurrent()

        assertFalse(lift.changes.last())
    }

    @Test
    fun `dock is lifted again when the card closes with the keyboard up after an earlier lift released`() = runTest {
        val lift = startLift(cardOpen = true, keyboardUp = true)
        lift.cardOpen.value = false
        runCurrent()
        lift.keyboardUp.value = false
        runCurrent()

        lift.cardOpen.value = true
        lift.keyboardUp.value = true
        runCurrent()
        lift.cardOpen.value = false
        runCurrent()

        assertEquals(listOf(true, false, true), lift.changes)
    }

    private fun TestScope.startLift(cardOpen: Boolean, keyboardUp: Boolean): KeyboardLift {
        val lift = KeyboardLift(MutableStateFlow(cardOpen), MutableStateFlow(keyboardUp))
        backgroundScope.launch {
            liftDockWhileKeyboardLeaves(
                cardOpen = lift.cardOpen,
                keyboardUp = lift.keyboardUp,
                onLift = { lift.changes += it },
            )
        }
        runCurrent()
        return lift
    }

    private class KeyboardLift(val cardOpen: MutableStateFlow<Boolean>, val keyboardUp: MutableStateFlow<Boolean>) {
        val changes = mutableListOf<Boolean>()
    }
}
