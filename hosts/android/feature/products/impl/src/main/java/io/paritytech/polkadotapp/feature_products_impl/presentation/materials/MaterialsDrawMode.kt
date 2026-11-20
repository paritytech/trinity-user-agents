package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ReadyLayer

sealed interface MaterialsDrawMode {
    data object Plain : MaterialsDrawMode

    data class Layers(val layers: ImmutableList<ReadyLayer>) : MaterialsDrawMode {
        // A material ID is a ByteArray, which a data class compares by reference.
        val key: List<Any> = layers.map { layer -> listOf(layer.material.toList(), layer.shader, layer.args) }
    }
}

fun materialsDrawMode(node: NodeMaterials?, reported: Boolean): MaterialsDrawMode =
    if (!reported && node is NodeMaterials.Ready && node.layers.isNotEmpty()) {
        MaterialsDrawMode.Layers(node.layers.toImmutableList())
    } else {
        MaterialsDrawMode.Plain
    }
