package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import uniffi.truapi.RendererNode

internal class RendererGestureOffsets {
    private var positions = emptyMap<String, JsGesturePosition>()

    fun receive(node: RendererNode): JsWidget? = when (node) {
        is RendererNode.SetOffset -> {
            positions[node.key]?.receive(node.x, node.y)
            null
        }
        else -> {
            val next = mutableMapOf<String, JsGesturePosition>()
            val widget = node.toJsWidget { key ->
                next.getOrPut(key) { positions[key] ?: JsGesturePosition() }
            }
            positions.filterKeys { it !in next }.values.forEach { it.detach() }
            next.values.forEach { it.receive(0, 0) }
            positions = next
            widget
        }
    }

    fun close() {
        positions.values.forEach { it.detach() }
        positions = emptyMap()
    }
}
