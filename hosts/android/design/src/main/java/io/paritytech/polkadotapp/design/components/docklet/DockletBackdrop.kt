package io.paritytech.polkadotapp.design.components.docklet

import android.graphics.RenderEffect
import android.graphics.RenderNode
import android.graphics.Shader
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.webkit.WebView
import androidx.annotation.RequiresApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.core.view.isVisible
import io.paritytech.polkadotapp.design.utils.blurRadiusPx

private const val BackdropScale = 0.5f

@RequiresApi(Build.VERSION_CODES.S)
@Stable
class DockletBackdrop internal constructor(private val source: View) {
    private val content = RenderNode("DockletBackdrop")
    private val layers = mutableListOf<BackdropLayer>()
    private val sourceLocation = IntArray(2)

    // A WebView inside an offscreen RenderEffect layer draws misplaced or black.
    internal var showsWebView by mutableStateOf(false)
        private set

    internal val preDrawListener = ViewTreeObserver.OnPreDrawListener {
        record()
        true
    }

    internal fun attach() = source.viewTreeObserver.addOnPreDrawListener(preDrawListener)

    internal fun detach() = source.viewTreeObserver.removeOnPreDrawListener(preDrawListener)

    internal fun register(layer: BackdropLayer) {
        layers += layer
    }

    internal fun unregister(layer: BackdropLayer) {
        layers -= layer
    }

    private fun record() {
        if (layers.isEmpty() || source.width == 0) return

        val webView = source.containsWebView()
        if (webView != showsWebView) showsWebView = webView
        if (layers.none { it.visible }) return

        content.setPosition(0, 0, source.width, source.height)
        val canvas = content.beginRecording()
        source.draw(canvas)
        content.endRecording()
        source.getLocationInWindow(sourceLocation)

        layers.forEach { it.record(content, sourceLocation) }
    }
}

private fun View.containsWebView(): Boolean {
    if (this is WebView) return true
    if (this !is ViewGroup) return false
    for (index in 0 until childCount) {
        val child = getChildAt(index)
        if (child.isVisible && child.containsWebView()) return true
    }
    return false
}

@RequiresApi(Build.VERSION_CODES.S)
@Stable
internal class BackdropLayer(private val radiusPx: () -> Float) {
    val node = RenderNode("DockletBackdropLayer")
    var coordinates: LayoutCoordinates? = null
    var width = 0
    var height = 0
    private var appliedRadius = 0f

    val visible: Boolean get() = radiusPx() > 0f

    fun record(content: RenderNode, sourceLocation: IntArray) {
        val layout = coordinates?.takeIf { it.isAttached } ?: return
        val radius = radiusPx()
        if (width == 0 || height == 0 || radius <= 0f) return

        if (radius != appliedRadius) {
            node.setRenderEffect(RenderEffect.createBlurEffect(radius, radius, Shader.TileMode.CLAMP))
            appliedRadius = radius
        }
        val position = layout.positionInWindow()
        node.setPosition(0, 0, (width * BackdropScale).toInt(), (height * BackdropScale).toInt())
        val canvas = node.beginRecording()
        canvas.scale(BackdropScale, BackdropScale)
        canvas.translate(sourceLocation[0] - position.x, sourceLocation[1] - position.y)
        canvas.drawRenderNode(content)
        node.endRecording()
    }
}

@Composable
fun rememberDockletBackdrop(source: View?): DockletBackdrop? {
    val backdrop = remember(source) {
        if (source != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) DockletBackdrop(source) else null
    }
    DisposableEffect(backdrop) {
        backdrop?.attach()
        onDispose { backdrop?.detach() }
    }
    return backdrop
}

@Composable
fun Modifier.dockletBackdrop(backdrop: DockletBackdrop?, shape: Shape, sigma: () -> Dp): Modifier =
    if (backdrop != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        backdropLayer(backdrop, shape, sigma)
    } else {
        this
    }

@RequiresApi(Build.VERSION_CODES.S)
@Composable
private fun Modifier.backdropLayer(backdrop: DockletBackdrop, shape: Shape, sigma: () -> Dp): Modifier {
    val density = LocalDensity.current.density
    val currentSigma by rememberUpdatedState(sigma)
    val layer = remember(backdrop, density) {
        BackdropLayer { if (backdrop.showsWebView) 0f else blurRadiusPx(currentSigma().value, density * BackdropScale) }
    }
    DisposableEffect(backdrop, layer) {
        backdrop.register(layer)
        onDispose { backdrop.unregister(layer) }
    }

    return this
        .onGloballyPositioned { layer.coordinates = it }
        .drawWithCache {
            layer.width = size.width.toInt()
            layer.height = size.height.toInt()
            val path = Path().apply { addOutline(shape.createOutline(size, layoutDirection, this@drawWithCache)) }
            onDrawBehind {
                if (layer.visible) {
                    clipPath(path) {
                        drawIntoCanvas { canvas ->
                            canvas.nativeCanvas.save()
                            canvas.nativeCanvas.scale(1 / BackdropScale, 1 / BackdropScale)
                            canvas.nativeCanvas.drawRenderNode(layer.node)
                            canvas.nativeCanvas.restore()
                        }
                    }
                }
            }
        }
}
