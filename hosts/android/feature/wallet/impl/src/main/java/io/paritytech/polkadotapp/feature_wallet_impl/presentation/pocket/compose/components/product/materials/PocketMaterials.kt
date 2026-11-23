package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.graphics.Bitmap
import androidx.compose.foundation.AndroidEmbeddedExternalSurface
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateSetOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsMaterialsRenderer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

internal class PocketMaterials(
    private val materials: CardMaterials,
    private val cardPicture: State<ImageBitmap?>,
    private val cardCoordinates: State<LayoutCoordinates?>,
) : JsMaterialsRenderer {
    // Per copy of the card: the list and the opened card each wait for their own surface's first frame.
    private val withSurface = mutableStateSetOf<Int>()

    fun shown(node: Int) {
        withSurface += node
    }

    fun hidden(node: Int) {
        withSurface -= node
    }

    fun showsSurface(node: Int): Boolean = node in withSurface

    @Composable
    override fun Node(modifier: Modifier, node: Int, children: @Composable () -> Unit) {
        val changes by materials.changes.collectAsState()
        val resolution by materials.resolution.collectAsState()
        val resolved by resolution.resolved.collectAsState()
        val allPlain by materials.allPlain.collectAsState()
        val draw = remember(node, changes, resolution, resolved, allPlain) { materials.draw(node) }
        val surfaceShown = draw is NodeDraw.Layers && showsSurface(node)
        var bounds by remember { mutableStateOf<Rect?>(null) }

        LaunchedEffect(node, resolved) { materials.reportCrashed(node) }

        Box(modifier, contentAlignment = Alignment.TopStart) {
            Box(
                Modifier
                    .matchParentSize()
                    .onGloballyPositioned { coordinates -> bounds = cardCoordinates.value?.localBoundingBoxOf(coordinates) },
            )

            if (draw is NodeDraw.Layers) {
                LayersSurface(
                    modifier = Modifier.matchParentSize(),
                    node = node,
                    draw = draw,
                    noiseSeed = resolved?.noiseSeed ?: 0u,
                    base = cardPicture.value?.let { picture -> bounds?.let { picture to it } },
                )
            }

            // Above the surface so presses inside the node reach its children, which stop drawing once it shows.
            Box(Modifier.drawWithContent { if (!surfaceShown) drawContent() }) { children() }
        }
    }

    @Composable
    private fun LayersSurface(
        modifier: Modifier,
        node: Int,
        draw: NodeDraw.Layers,
        noiseSeed: UInt,
        base: Pair<ImageBitmap, Rect>?,
    ) {
        val lifecycleOwner = LocalLifecycleOwner.current
        val surface = remember(this, node) { materials.surface(node) { shown(node) } }
        val prepared by remember(node, draw, noiseSeed) { materials.prepared(node, draw, noiseSeed) }.collectAsState()

        DisposableEffect(surface) {
            onDispose {
                surface.release()
                hidden(node)
            }
        }

        DisposableEffect(surface, lifecycleOwner) {
            val observer = LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> surface.resume()
                    Lifecycle.Event.ON_STOP -> surface.pause()
                    else -> Unit
                }
            }
            lifecycleOwner.lifecycle.addObserver(observer)
            onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
        }

        LaunchedEffect(surface, prepared) {
            prepared?.let { surface.setLayers(it.layers, it.images, it.noise) }
        }

        LaunchedEffect(surface, base) {
            val (picture, rect) = base ?: return@LaunchedEffect
            val crop = withContext(Dispatchers.Default) { crop(picture, rect) } ?: return@LaunchedEffect
            surface.setBase(crop)
        }

        // A replaced surface needs its own external surface: the old one hands its Surface over only once.
        key(surface) {
            AndroidEmbeddedExternalSurface(modifier = modifier, isOpaque = false) {
                onSurface { target, width, height ->
                    surface.attach(target, width, height)
                    target.onChanged { newWidth, newHeight -> surface.resize(newWidth, newHeight) }
                    target.onDestroyed { surface.detach() }
                }
            }
        }
    }

    private fun crop(picture: ImageBitmap, rect: Rect): Bitmap? {
        val drawn = picture.asAndroidBitmap()
        val card = drawn.takeIf { it.config != Bitmap.Config.HARDWARE } ?: drawn.copy(Bitmap.Config.ARGB_8888, false)
        val left = rect.left.roundToInt().coerceIn(0, card.width)
        val top = rect.top.roundToInt().coerceIn(0, card.height)
        val width = rect.width.roundToInt().coerceAtMost(card.width - left)
        val height = rect.height.roundToInt().coerceAtMost(card.height - top)
        if (width <= 0 || height <= 0) return null
        return Bitmap.createBitmap(card, left, top, width, height)
    }
}
