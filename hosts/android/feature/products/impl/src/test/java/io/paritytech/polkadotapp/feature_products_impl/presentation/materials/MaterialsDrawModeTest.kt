package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import kotlinx.collections.immutable.persistentListOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialShader
import uniffi.truapi.NodeMaterials
import uniffi.truapi.Parameter
import uniffi.truapi.ReadyLayer

class MaterialsDrawModeTest {
    private val shader = MaterialShader(
        vertex = "void main() {}",
        fragment = "void main() {}",
        uniforms = emptyList(),
        vertexBlockSize = 0u,
        fragmentBlockSize = 0u,
        textures = emptyList(),
        readsTime = true,
        readsMotion = false,
    )

    private fun layer(strength: Float = 1f, shader: MaterialShader = this.shader) =
        ReadyLayer(material = ByteArray(32) { 7 }, shader = shader, args = listOf(Parameter.Float(strength.toRawBits().toUInt())))

    @Test
    fun `a node draws its layers only once they are ready and nothing failed on the device`() {
        val ready = NodeMaterials.Ready(layers = listOf(layer()))

        assertEquals(
            listOf(
                MaterialsDrawMode.Plain,
                MaterialsDrawMode.Plain,
                MaterialsDrawMode.Plain,
                MaterialsDrawMode.Layers(persistentListOf(ready.layers.single())),
                MaterialsDrawMode.Plain,
                MaterialsDrawMode.Plain,
            ),
            listOf(
                materialsDrawMode(null, reported = false),
                materialsDrawMode(NodeMaterials.Loading, reported = false),
                materialsDrawMode(NodeMaterials.Plain(MaterialFailure.UNSUPPORTED), reported = false),
                materialsDrawMode(ready, reported = false),
                materialsDrawMode(ready, reported = true),
                materialsDrawMode(NodeMaterials.Ready(layers = emptyList()), reported = false),
            ),
        )
    }

    @Test
    fun `the same layers from a new tree keep the surface, changed arguments restart it`() {
        val drawn = MaterialsDrawMode.Layers(persistentListOf(layer()))

        assertEquals(drawn.key, MaterialsDrawMode.Layers(persistentListOf(layer())).key)
        assertNotEquals(drawn.key, MaterialsDrawMode.Layers(persistentListOf(layer(strength = 2f))).key)
        assertNotEquals(drawn.key, MaterialsDrawMode.Layers(persistentListOf(layer(shader = shader.copy(fragment = "void main() { }")))).key)
    }
}
