package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.MaterialsResolution
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialShader
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ReadyLayer
import uniffi.truapi.ResolvedMaterials

class CardMaterialsTest {
    private val tint = ByteArray(32) { 1 }
    private val layers = listOf(ReadyLayer(tint, shader(), emptyList()))
    private val resolved = MutableStateFlow<ResolvedMaterials?>(null)
    private val resolution = mockk<MaterialsResolution>(relaxed = true) { every { resolved } returns this@CardMaterialsTest.resolved }
    private val guard = mockk<CompileGuard> { every { crashed(any()) } returns false }
    private val allPlain = MutableStateFlow(false)
    private val materials = CardMaterials(resolution, guard, allPlain)

    private fun nodes(vararg nodes: NodeMaterials) {
        resolved.value = ResolvedMaterials(nodes.toList(), noiseSeed = 7u)
    }

    @Test
    fun `a node draws its layers once ready, and its children while loading or plain`() {
        val drawn = listOf(
            NodeMaterials.Loading,
            NodeMaterials.Ready(layers),
            NodeMaterials.Plain(MaterialFailure.NOT_FOUND),
        ).map { node ->
            nodes(node)
            materials.draw(0)
        }

        assertEquals(listOf(NodeDraw.Plain, NodeDraw.Layers(layers), NodeDraw.Plain), drawn)
    }

    @Test
    fun `a failed node is reported once and drawn plain, and the other nodes keep drawing`() {
        nodes(NodeMaterials.Ready(layers), NodeMaterials.Ready(layers))

        materials.failed(0, tint, MaterialFailure.UNSUPPORTED)
        materials.failed(0, tint, MaterialFailure.UNSUPPORTED)

        verify(exactly = 1) { resolution.report(0u, tint, MaterialFailure.UNSUPPORTED) }
        assertEquals(NodeDraw.Plain, materials.draw(0))
        assertEquals(NodeDraw.Layers(layers), materials.draw(1))
    }

    // The RFC: stepping down for performance, heat or power saving is not a failure, and is not reported.
    @Test
    fun `a node too slow to draw, or a strained device, goes plain without a report`() {
        nodes(NodeMaterials.Ready(layers), NodeMaterials.Ready(layers))

        materials.tooSlow(0)
        val slowDrawn = listOf(materials.draw(0), materials.draw(1))
        allPlain.value = true

        assertEquals(listOf(NodeDraw.Plain, NodeDraw.Layers(layers)), slowDrawn)
        assertEquals(NodeDraw.Plain, materials.draw(1))
        verify(exactly = 0) { resolution.report(any(), any(), any()) }
    }

    // A material that took the app down while compiling is not compiled again on this device.
    @Test
    fun `a material that crashed a compile draws plain and is reported unsupported once`() {
        every { guard.crashed(tint) } returns true
        nodes(NodeMaterials.Ready(layers))

        val drawn = listOf(materials.draw(0), materials.draw(0))
        repeat(2) { materials.reportCrashed(0) }

        assertEquals(listOf(NodeDraw.Plain, NodeDraw.Plain), drawn)
        verify(exactly = 1) { resolution.report(0u, tint, MaterialFailure.UNSUPPORTED) }
    }

    private fun shader() = MaterialShader(
        vertex = "",
        fragment = "",
        uniforms = emptyList(),
        vertexBlockSize = 0u,
        fragmentBlockSize = 0u,
        textures = emptyList(),
        readsTime = false,
        readsMotion = false,
    )
}
