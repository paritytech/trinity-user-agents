package io.paritytech.polkadotapp.common.presentation.tabbar

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ForceShowTabBarTest {
    @get:Rule
    val compose = createComposeRule()

    private val holder = TabBarVisibilityHolder()
    private val host = FakeLifecycleOwner()
    private var pinned by mutableStateOf(true)

    @Test
    fun aScreenThatIsOnlyStartedGetsNoBar() {
        pinBar()
        moveHostTo(Lifecycle.State.STARTED)

        assertTrue(holder.hidden.value)
    }

    @Test
    fun theBarAppearsOnceTheScreenResumes() {
        pinBar()
        moveHostTo(Lifecycle.State.RESUMED)

        assertFalse(holder.hidden.value)
        assertTrue(holder.forced.value)
    }

    @Test
    fun theBarStaysWhenAResumedScreenPauses() {
        pinBar()
        moveHostTo(Lifecycle.State.RESUMED)
        moveHostTo(Lifecycle.State.CREATED)

        assertFalse(holder.hidden.value)
    }

    @Test
    fun theBarLeavesWithTheScreen() {
        pinBar()
        moveHostTo(Lifecycle.State.RESUMED)
        unpinBar()

        assertTrue(holder.hidden.value)
    }

    @Test
    fun aPreviewThatNeverResumesLeavesNothingPinned() {
        pinBar()
        moveHostTo(Lifecycle.State.STARTED)
        unpinBar()
        moveHostTo(Lifecycle.State.RESUMED)

        assertTrue(holder.hidden.value)
    }

    private fun pinBar() {
        compose.setContent {
            if (pinned) {
                CompositionLocalProvider(
                    LocalLifecycleOwner provides host,
                    LocalTabBarVisibility provides holder,
                ) {
                    ForceShowTabBar()
                }
            }
        }
        compose.waitForIdle()
    }

    private fun moveHostTo(state: Lifecycle.State) {
        compose.runOnUiThread { host.registry.currentState = state }
        compose.waitForIdle()
    }

    private fun unpinBar() {
        pinned = false
        compose.waitForIdle()
    }

    private class FakeLifecycleOwner : LifecycleOwner {
        val registry = LifecycleRegistry(this)
        override val lifecycle: Lifecycle get() = registry
    }
}
