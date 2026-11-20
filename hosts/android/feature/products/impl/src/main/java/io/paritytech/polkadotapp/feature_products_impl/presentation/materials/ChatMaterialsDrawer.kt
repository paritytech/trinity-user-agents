package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsMaterialsDrawer
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import uniffi.truapi.ResolvedMaterials

class ChatMaterialsDrawer(
    private val materials: ResolvedMaterials?,
    private val reportedNodes: ImmutableSet<Int>,
    private val onFailure: (node: Int, failure: MaterialDrawFailure) -> Unit,
) : JsMaterialsDrawer {
    @Composable
    override fun Draw(modifier: Modifier, index: Int, content: @Composable () -> Unit) {
        val mode = remember(materials, index, reportedNodes) {
            materialsDrawMode(materials?.nodes?.getOrNull(index), index in reportedNodes)
        }
        val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
        val base = rememberGraphicsLayer()
        // Not snapshot state: a count read and written while drawing would invalidate the draw that wrote it.
        val baseDraws = remember { MutableStateFlow(0) }
        var surfaceShowsFrame by remember { mutableStateOf(false) }
        Box(modifier = modifier) {
            Box(
                // Its own layer, so the surface invalidating the shared one does not record the base again.
                modifier = Modifier.graphicsLayer().drawWithContent {
                    base.record { this@drawWithContent.drawContent() }
                    baseDraws.update { it + 1 }
                    if (!surfaceShowsFrame) drawLayer(base)
                },
            ) {
                content()
            }
            if (mode is MaterialsDrawMode.Layers && materials != null && lifecycle.isAtLeast(Lifecycle.State.STARTED)) {
                key(mode.key, materials.noiseSeed) {
                    MaterialsSurface(
                        modifier = Modifier.matchParentSize(),
                        layers = mode.layers,
                        noiseSeed = materials.noiseSeed,
                        base = base,
                        baseDraws = baseDraws,
                        onFrameShown = { surfaceShowsFrame = true },
                        onFailure = { failure -> onFailure(index, failure) },
                    )
                    DisposableEffect(Unit) {
                        onDispose { surfaceShowsFrame = false }
                    }
                }
            }
        }
    }
}
