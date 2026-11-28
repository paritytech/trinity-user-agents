package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import io.parity.truapi.MaterialsResolution
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.truapi.ImageRef
import uniffi.truapi.ImageSource
import uniffi.truapi.MaterialFailure
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ReadyLayer
import uniffi.truapi.ResolvedMaterials
import uniffi.truapi.materialNoise

internal sealed interface NodeDraw {
    data object Plain : NodeDraw

    class Layers(val layers: List<ReadyLayer>) : NodeDraw {
        override fun equals(other: Any?): Boolean = other is Layers && layers.drawsSameAs(other.layers)

        override fun hashCode(): Int = layers.fold(0) { hash, layer -> hash * HASH_FACTOR + layer.material.contentHashCode() }

        private companion object {
            const val HASH_FACTOR = 31
        }
    }
}

internal class PreparedLayers(val layers: List<ReadyLayer>, val images: Map<Pair<Int, Int>, Bitmap>, val noise: ByteArray)

// One per card, kept across the card's resolutions and copies, so a node keeps its last output while the next
// face resolves and a dropped node stays dropped when its card scrolls away and back. It runs in the card's
// ViewModel scope, which loads the layers' images and reports failures, so the surfaces only draw. Main thread only.
internal class CardMaterials(
    resolution: MaterialsResolution,
    private val gl: MaterialsGl,
    private val loadImage: suspend (ImageSource) -> Result<Bitmap>,
    private val scope: CoroutineScope,
) {
    private val shown = MutableStateFlow(resolution)
    private val lastLayers = mutableMapOf<Int, NodeDraw.Layers>()
    private val dropped = mutableMapOf<Int, NodeDraw.Layers?>()
    private val images = mutableMapOf<Int, Map<ImageSource, Bitmap>>()
    private val preparations = mutableMapOf<Int, Preparation>()
    private var noise: Pair<UInt, ByteArray>? = null

    val resolution: StateFlow<MaterialsResolution> = shown.asStateFlow()

    val changes = MutableStateFlow(0)

    val allPlain: StateFlow<Boolean> get() = gl.allPlain

    fun show(resolution: MaterialsResolution) {
        shown.value = resolution
    }

    fun draw(node: Int): NodeDraw {
        if (gl.allPlain.value) return NodeDraw.Plain
        val draw = when (val materials = nodeMaterials(node)) {
            is NodeMaterials.Ready ->
                if (crashedLayer(materials) != null) NodeDraw.Plain else NodeDraw.Layers(materials.layers)
            NodeMaterials.Loading -> lastLayers[node] ?: NodeDraw.Plain
            is NodeMaterials.Plain, null -> NodeDraw.Plain
        }
        if (node in dropped) {
            if (draw == NodeDraw.Plain || dropped[node] == draw) return NodeDraw.Plain
            dropped.remove(node)
        }
        if (draw is NodeDraw.Layers) lastLayers[node] = draw
        return draw
    }

    fun reportCrashed(node: Int) {
        val ready = nodeMaterials(node) as? NodeMaterials.Ready ?: return
        val crashed = crashedLayer(ready) ?: return
        failed(node, crashed.material, MaterialFailure.UNSUPPORTED)
    }

    fun failed(node: Int, material: ByteArray?, reason: MaterialFailure) {
        if (node in dropped) return
        drop(node)
        resolution.value.report(node.toUInt(), material, reason)
    }

    // Stepping down for performance is not a failure, so the product is not told.
    fun tooSlow(node: Int) {
        if (node !in dropped) drop(node)
    }

    fun surface(node: Int, onFirstFrame: () -> Unit): MaterialsSurface {
        val main = Handler(Looper.getMainLooper())
        return MaterialsSurface(
            gl = gl,
            onFirstFrame = { main.post(onFirstFrame) },
            onFailure = { material, reason -> main.post { failed(node, material, reason) } },
            onTooSlow = { main.post { tooSlow(node) } },
        )
    }

    // Every copy of the card shares one load of a node's images per set of layers.
    fun prepared(node: Int, draw: NodeDraw.Layers, noiseSeed: UInt): StateFlow<PreparedLayers?> {
        preparations[node]?.takeIf { it.draw == draw && it.noiseSeed == noiseSeed }?.let { return it.ready }
        preparations.remove(node)?.job?.cancel()
        val ready = MutableStateFlow<PreparedLayers?>(null)
        val job = scope.launch { ready.value = prepare(node, draw, noiseSeed) }
        preparations[node] = Preparation(draw, noiseSeed, job, ready)
        return ready
    }

    private suspend fun prepare(node: Int, draw: NodeDraw.Layers, noiseSeed: UInt): PreparedLayers? {
        val byArgument = mutableMapOf<Pair<Int, Int>, Bitmap>()
        val loaded = mutableMapOf<ImageSource, Bitmap>()
        draw.layers.forEachIndexed { index, layer ->
            layer.args.forEachIndexed { argument, parameter ->
                val source = (parameter.imageRef() as? ImageRef.Source)?.v1 ?: return@forEachIndexed
                val image = images[node]?.get(source) ?: loadImage(source).getOrElse { failure ->
                    failed(node, layer.material, (failure as? MaterialImageRefused)?.reason ?: MaterialFailure.NOT_FOUND)
                    return null
                }
                byArgument[index to argument] = image
                loaded[source] = image
            }
        }
        images[node] = loaded
        return PreparedLayers(draw.layers, byArgument, noise(noiseSeed))
    }

    private suspend fun noise(seed: UInt): ByteArray {
        noise?.let { (madeFor, image) -> if (madeFor == seed) return image }
        return withContext(Dispatchers.Default) { materialNoise(seed) }.also { noise = seed to it }
    }

    private fun nodeMaterials(node: Int): NodeMaterials? {
        val resolved = resolution.value.resolved.value ?: return NodeMaterials.Loading
        return resolved.nodes.getOrNull(node)
    }

    private fun crashedLayer(ready: NodeMaterials.Ready): ReadyLayer? = ready.layers.firstOrNull { gl.guard.crashed(it.material) }

    private fun drop(node: Int) {
        dropped[node] = (nodeMaterials(node) as? NodeMaterials.Ready)?.let { NodeDraw.Layers(it.layers) } ?: lastLayers[node]
        changes.value++
    }

    private class Preparation(val draw: NodeDraw.Layers, val noiseSeed: UInt, val job: Job, val ready: StateFlow<PreparedLayers?>)
}

/** Holds one card's materials across its faces, loading their images in [scope] through [loadImage]. */
class CardMaterialsSlot(
    private val gl: MaterialsGl,
    private val loadImage: suspend (ImageSource) -> Result<Bitmap>,
    private val scope: CoroutineScope,
) {
    private var held: CardMaterials? = null
    private var drawsMaterials = false

    internal fun of(resolution: MaterialsResolution): CardMaterials =
        held?.also { it.show(resolution) } ?: CardMaterials(resolution, gl, loadImage, scope).also { held = it }

    // Only a node that can draw layers needs GL; until the next resolution settles the card keeps its path.
    internal fun drawsMaterials(resolved: ResolvedMaterials?): Boolean {
        if (resolved != null) drawsMaterials = resolved.nodes.any { it !is NodeMaterials.Plain }
        return drawsMaterials
    }
}
