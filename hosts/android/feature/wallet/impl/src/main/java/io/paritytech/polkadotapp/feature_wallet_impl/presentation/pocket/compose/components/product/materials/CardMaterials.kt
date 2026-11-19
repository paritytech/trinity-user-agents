package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.graphics.Bitmap
import io.parity.truapi.MaterialsResolution
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import uniffi.truapi.ImageRef
import uniffi.truapi.ImageSource
import uniffi.truapi.MaterialFailure
import uniffi.truapi.NodeMaterials
import uniffi.truapi.Parameter
import uniffi.truapi.ReadyLayer
import uniffi.truapi.ResolvedMaterials
import uniffi.truapi.materialNoise

internal sealed interface NodeDraw {
    data object Plain : NodeDraw

    // The generated layer compares its material and inline images by reference, and the core sends new
    // ones on every update, so equal layers are compared by content.
    class Layers(val layers: List<ReadyLayer>) : NodeDraw {
        override fun equals(other: Any?): Boolean = other is Layers && layers.size == other.layers.size &&
            layers.zip(other.layers).all { (mine, theirs) -> mine.sameAs(theirs) }

        override fun hashCode(): Int = layers.fold(0) { hash, layer -> hash * HASH_FACTOR + layer.material.contentHashCode() }

        private fun ReadyLayer.sameAs(other: ReadyLayer): Boolean =
            material.contentEquals(other.material) && shader == other.shader &&
                args.size == other.args.size && args.zip(other.args).all { (mine, theirs) -> mine.sameAs(theirs) }

        private fun Parameter.sameAs(other: Parameter): Boolean {
            val mine = (imageRef() as? ImageRef.Source)?.v1 as? ImageSource.Inline
            val theirs = (other.imageRef() as? ImageRef.Source)?.v1 as? ImageSource.Inline
            return if (mine != null && theirs != null) mine.v1.contentEquals(theirs.v1) else this == other
        }

        private companion object {
            const val HASH_FACTOR = 31
        }
    }
}

// One per card, kept across the card's resolutions and copies, so a node keeps its last output while the next
// face resolves and a dropped node stays dropped when its card scrolls away and back. Main thread only.
internal class CardMaterials(
    resolution: MaterialsResolution,
    private val guard: CompileGuard,
    private val allPlain: StateFlow<Boolean>,
) {
    private val shown = MutableStateFlow(resolution)
    private val lastLayers = mutableMapOf<Int, NodeDraw.Layers>()
    private val dropped = mutableMapOf<Int, NodeDraw.Layers?>()
    private val images = mutableMapOf<Int, Map<ImageSource, Bitmap>>()
    private var noise: Pair<UInt, ByteArray>? = null

    val resolution: StateFlow<MaterialsResolution> = shown.asStateFlow()

    val changes = MutableStateFlow(0)

    fun show(resolution: MaterialsResolution) {
        shown.value = resolution
    }

    fun draw(node: Int): NodeDraw {
        if (allPlain.value) return NodeDraw.Plain
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

    fun image(node: Int, source: ImageSource): Bitmap? = images[node]?.get(source)

    fun keepImages(node: Int, loaded: Map<ImageSource, Bitmap>) {
        images[node] = loaded
    }

    suspend fun noise(seed: UInt): ByteArray {
        noise?.let { (madeFor, image) -> if (madeFor == seed) return image }
        return withContext(Dispatchers.Default) { materialNoise(seed) }.also { noise = seed to it }
    }

    private fun nodeMaterials(node: Int): NodeMaterials? {
        val resolved = resolution.value.resolved.value ?: return NodeMaterials.Loading
        return resolved.nodes.getOrNull(node)
    }

    private fun crashedLayer(ready: NodeMaterials.Ready): ReadyLayer? = ready.layers.firstOrNull { guard.crashed(it.material) }

    private fun drop(node: Int) {
        dropped[node] = (nodeMaterials(node) as? NodeMaterials.Ready)?.let { NodeDraw.Layers(it.layers) } ?: lastLayers[node]
        changes.value++
    }
}

class CardMaterialsSlot {
    private var held: CardMaterials? = null
    private var drawsMaterials = false

    internal fun of(resolution: MaterialsResolution, guard: CompileGuard, allPlain: StateFlow<Boolean>): CardMaterials =
        held?.also { it.show(resolution) } ?: CardMaterials(resolution, guard, allPlain).also { held = it }

    // Only a node that can draw layers needs GL; until the next resolution settles the card keeps its path.
    internal fun drawsMaterials(resolved: ResolvedMaterials?): Boolean {
        if (resolved != null) drawsMaterials = resolved.nodes.any { it !is NodeMaterials.Plain }
        return drawsMaterials
    }
}
