package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.os.Trace
import androidx.compose.runtime.AbstractApplier
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composition
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.node.GlobalPositionAwareModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class MaskObserverTest {
    @Before
    fun stubAndroidTracing() {
        mockkStatic(Trace::class)
        justRun { Trace.beginSection(any()) }
        justRun { Trace.endSection() }
    }

    @After
    fun restoreAndroidTracing() {
        unmockkStatic(Trace::class)
    }

    @Test
    fun `a later lift counts the new trees shapes as well as the followed stroke`() = runTest {
        val channel = JsMaskShapes()
        val trail = JsPanTrail()
        val mask = mutableStateOf(
            JsModifier.Mask(
                key = "surface",
                follow = JsMaskFollow("pan", 4700),
                clearedShare = JsClearedShare(580, "covered"),
                channel = channel,
                trail = trail,
            ),
        )
        val actions = mutableListOf<String>()
        val coordinates = mockk<LayoutCoordinates> {
            every { size } returns IntSize(193, 98)
            every { localToRoot(any()) } returns Offset.Zero
        }
        val clock = BroadcastFrameClock()
        val recomposer = Recomposer(coroutineContext + clock)
        val composition = Composition(EmptyApplier(), recomposer)
        val running = launch(clock) { recomposer.runRecomposeAndApplyChanges() }
        var frame = 0L
        fun drawFrame() {
            Snapshot.sendApplyNotifications()
            runCurrent()
            clock.sendFrame(++frame * 16_000_000L)
            runCurrent()
        }

        try {
            composition.setContent {
                CompositionLocalProvider(
                    LocalDensity provides Density(1f),
                    LocalLayoutDirection provides LayoutDirection.Ltr,
                    LocalJsEventHandler provides { action, _ -> actions += action },
                ) {
                    val modifier = mask.value.maskModifier()
                    SideEffect {
                        modifier.foldIn<GlobalPositionAwareModifierNode?>(null) { found, element ->
                            found ?: (element as? ModifierNodeElement<*>)?.create() as? GlobalPositionAwareModifierNode
                        }?.onGloballyPositioned(coordinates)
                    }
                }
            }
            drawFrame()
            drawFrame()
            trail.touch(true)
            trail.begin(Offset(15f, 25f))
            trail.extend(Offset(175f, 25f))
            drawFrame()
            trail.touch(false)
            drawFrame()

            val replacement = mask.value.copy(shapes = requireNotNull(channel.source).invoke().shapes)
            channel.treeArrived(replacement)
            trail.reset()
            mask.value = replacement
            drawFrame()
            drawFrame()

            trail.touch(true)
            trail.begin(Offset(15f, 75f))
            trail.extend(Offset(175f, 75f))
            drawFrame()
            trail.touch(false)
            drawFrame()

            assertEquals(listOf("covered"), actions)
        } finally {
            composition.dispose()
            recomposer.cancel()
            running.cancelAndJoin()
        }
    }

    private class EmptyApplier : AbstractApplier<Unit>(Unit) {
        override fun insertTopDown(index: Int, instance: Unit) = Unit
        override fun insertBottomUp(index: Int, instance: Unit) = Unit
        override fun remove(index: Int, count: Int) = Unit
        override fun move(from: Int, to: Int, count: Int) = Unit
        override fun onClear() = Unit
    }
}
