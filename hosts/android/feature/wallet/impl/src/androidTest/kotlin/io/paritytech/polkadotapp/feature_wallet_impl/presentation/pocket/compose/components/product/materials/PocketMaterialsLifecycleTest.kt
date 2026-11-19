package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.MaterialsResolution
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ResolvedMaterials
import java.util.concurrent.CopyOnWriteArrayList

// A card that is not seen must not keep the GPU busy: its loop stops when the app stops and when
// the node leaves composition, as it does when the card scrolls away or the tab changes.
@RunWith(AndroidJUnit4::class)
class PocketMaterialsLifecycleTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val surfaces = CopyOnWriteArrayList<MaterialsSurface>()

    @Before
    fun startGl() = MaterialsGl.start(context)

    @Test
    fun renderLoopStopsWhenTheNodeLeavesCompositionOrTheLifecycleStops() {
        val owner = object : LifecycleOwner {
            val registry = LifecycleRegistry(this)
            override val lifecycle: Lifecycle = registry
        }
        compose.runOnUiThread { owner.registry.currentState = Lifecycle.State.RESUMED }
        val resolution = mockk<MaterialsResolution>(relaxed = true) {
            every { resolved } returns MutableStateFlow(
                ResolvedMaterials(listOf(NodeMaterials.Ready(listOf(layer(7, TIME, readsTime = true)))), noiseSeed = 1u),
            )
        }
        val materials = CardMaterials(resolution, MaterialsGl.guard, MaterialsGl.allPlain)
        val picture = mutableStateOf<ImageBitmap?>(card())
        val cardCoordinates = mutableStateOf<LayoutCoordinates?>(null)
        val renderer = PocketMaterials(materials, picture, cardCoordinates) { node, onFirstFrame ->
            nodeSurface(materials, node, onFirstFrame).also { surfaces += it }
        }
        var composed by mutableStateOf(true)

        compose.setContent {
            CompositionLocalProvider(LocalLifecycleOwner provides owner) {
                Box(Modifier.size(80.dp).onGloballyPositioned { cardCoordinates.value = it }) {
                    if (composed) renderer.Node(Modifier.size(40.dp), 0) { Box(Modifier.size(40.dp)) }
                }
            }
        }
        compose.waitUntil(timeoutMillis = 10_000) { framesDrawn() > 2 }

        compose.runOnUiThread { owner.registry.currentState = Lifecycle.State.CREATED }
        assertEquals("frames drawn while the app is stopped", 0, framesOver(QUIET_MILLIS))

        compose.runOnUiThread { owner.registry.currentState = Lifecycle.State.RESUMED }
        compose.waitUntil(timeoutMillis = 10_000) { framesOver(100) > 0 }

        compose.runOnUiThread { composed = false }
        compose.waitForIdle()
        assertEquals("frames drawn after the node left composition", 0, framesOver(QUIET_MILLIS))
    }

    private fun framesDrawn() = surfaces.sumOf { it.framesDrawn }

    private fun framesOver(millis: Long): Int {
        Thread.sleep(SETTLE_MILLIS)
        val before = framesDrawn()
        Thread.sleep(millis)
        return framesDrawn() - before
    }

    private fun card(): ImageBitmap =
        Bitmap.createBitmap(CARD_PIXELS, CARD_PIXELS, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.GRAY) }.asImageBitmap()

    private companion object {
        const val TIME = "vec4 material(vec2 uv){return vec4(fract(sl_time), texture(r_input, uv).g, 0.0, 1.0);}"
        const val CARD_PIXELS = 400
        const val QUIET_MILLIS = 500L
        const val SETTLE_MILLIS = 100L
    }
}
