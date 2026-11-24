package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.keyedMasks
import uniffi.truapi.RendererNode

/**
 * What changes on keyed nodes between trees: gesture offsets (`SetOffset`) and shapes added to masks (`ExtendMask`),
 * which a worker sends; and the finger's strokes on keyed location pans, which the host keeps for the masks that
 * follow them. An update reaches its node straight away, with no tree to map; a new tree replaces every update,
 * because the tree always wins.
 */
internal class RendererGestureOffsets {
    private var positions = emptyMap<String, JsGesturePosition>()
    private var trails = emptyMap<String, JsPanTrail>()
    private var masks = emptyMap<String, JsMaskShapes>()

    fun receive(node: RendererNode): JsWidget? = when (node) {
        is RendererNode.SetOffset -> {
            positions[node.key]?.receive(node.x, node.y)
            null
        }
        is RendererNode.ExtendMask -> {
            masks[node.key]?.extend(node.shapes.map { it.toJs() })
            // The worker's answer to a paced move, whichever gesture is under way, even with no shapes.
            positions.values.forEach { it.acknowledge() }
            null
        }
        else -> {
            val next = mutableMapOf<String, JsGesturePosition>()
            val nextTrails = mutableMapOf<String, JsPanTrail>()
            val nextMasks = mutableMapOf<String, JsMaskShapes>()
            val widget = node.toJsWidget(
                RendererKeyed(
                    gesturePosition = { key -> next.getOrPut(key) { positions[key] ?: JsGesturePosition() } },
                    panTrail = { key -> nextTrails.getOrPut(key) { trails[key] ?: JsPanTrail() } },
                    maskShapes = { key -> nextMasks.getOrPut(key) { masks[key] ?: JsMaskShapes() } },
                ),
            )
            positions.filterKeys { it !in next }.values.forEach { it.detach() }
            next.values.forEach { it.receive(0, 0) }
            // Each mask starts its fade from what it drew, before the strokes it followed are dropped.
            val keyed = widget.keyedMasks()
            nextMasks.forEach { (key, mask) -> mask.treeArrived(keyed[key]) }
            nextTrails.values.forEach { it.reset() }
            positions = next
            trails = nextTrails
            masks = nextMasks
            widget
        }
    }

    fun close() {
        positions.values.forEach { it.detach() }
        positions = emptyMap()
        trails = emptyMap()
        masks = emptyMap()
    }
}
