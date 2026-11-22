package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsCoverCells
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import uniffi.truapi.RendererNode

/**
 * What a worker changes on its keyed nodes between trees: gesture offsets (`SetOffset`) and cleared cover cells
 * (`ClearCells`), and the finger's strokes on keyed location pans, which a cover's brush draws with those cells. An
 * update reaches its node straight away, with no tree to map; a new tree replaces every update, because the tree
 * always wins.
 */
internal class RendererGestureOffsets {
    private var positions = emptyMap<String, JsGesturePosition>()
    private var covers = emptyMap<String, JsCoverCells>()
    private var trails = emptyMap<String, JsPanTrail>()

    fun receive(node: RendererNode): JsWidget? = when (node) {
        is RendererNode.SetOffset -> {
            positions[node.key]?.receive(node.x, node.y)
            null
        }
        is RendererNode.ClearCells -> {
            covers[node.key]?.clear(node.cells.map { it.toInt() })
            // The worker's answer to a paced move, whichever gesture is under way.
            positions.values.forEach { it.acknowledge() }
            null
        }
        else -> {
            val next = mutableMapOf<String, JsGesturePosition>()
            val nextCovers = mutableMapOf<String, JsCoverCells>()
            val nextTrails = mutableMapOf<String, JsPanTrail>()
            val widget = node.toJsWidget(
                gesturePosition = { key -> next.getOrPut(key) { positions[key] ?: JsGesturePosition() } },
                coverCells = { key -> nextCovers.getOrPut(key) { covers[key] ?: JsCoverCells() } },
                panTrail = { key -> nextTrails.getOrPut(key) { trails[key] ?: JsPanTrail() } },
            )
            positions.filterKeys { it !in next }.values.forEach { it.detach() }
            next.values.forEach { it.receive(0, 0) }
            nextCovers.values.forEach { it.reset() }
            nextTrails.values.forEach { it.reset() }
            positions = next
            covers = nextCovers
            trails = nextTrails
            widget
        }
    }

    fun close() {
        positions.values.forEach { it.detach() }
        positions = emptyMap()
        covers = emptyMap()
        trails = emptyMap()
    }
}
