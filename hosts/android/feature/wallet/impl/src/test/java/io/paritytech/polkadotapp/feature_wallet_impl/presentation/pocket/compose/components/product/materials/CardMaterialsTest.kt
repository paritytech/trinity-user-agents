package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.MaterialsResolution
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialShader
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ReadyLayer
import uniffi.truapi.ResolvedMaterials
import uniffi.truapi.materialNoise

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

    // A face redrawn with new arguments loads again; the node keeps its look rather than flashing plain.
    @Test
    fun `a node loading again keeps the layers it last drew`() {
        nodes(NodeMaterials.Ready(layers))
        materials.draw(0)
        nodes(NodeMaterials.Loading)

        assertEquals(NodeDraw.Layers(layers), materials.draw(0))
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

    // The RFC: stepping down for performance is not a failure, and is not reported.
    @Test
    fun `a node too slow to draw goes plain without a report`() {
        nodes(NodeMaterials.Ready(layers))

        materials.tooSlow(0)

        assertEquals(NodeDraw.Plain, materials.draw(0))
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

    @Test
    fun `heat or power saving draws every node plain without a report`() {
        nodes(NodeMaterials.Ready(layers), NodeMaterials.Ready(layers))
        allPlain.value = true

        assertEquals(listOf(NodeDraw.Plain, NodeDraw.Plain), listOf(materials.draw(0), materials.draw(1)))
        verify(exactly = 0) { resolution.report(any(), any(), any()) }
    }

    @Test
    fun `the noise image is made once per body`() = runBlocking<Unit> {
        val first = materials.noise(7u)

        assertArrayEquals(materialNoise(7u), first)
        assertSame(first, materials.noise(7u))
    }

    // A product that redraws its face gets a new resolution; the node keeps drawing what it drew
    // until the new one settles, rather than flashing plain.
    @Test
    fun `a card keeps its last layers while its next resolution loads`() {
        val slot = CardMaterialsSlot()
        nodes(NodeMaterials.Ready(layers))
        val card = slot.of(resolution, guard, allPlain)
        card.draw(0)

        val next = mockk<MaterialsResolution>(relaxed = true) { every { resolved } returns MutableStateFlow(null) }

        assertSame(card, slot.of(next, guard, allPlain))
        assertSame(next, card.resolution.value)
        assertEquals(NodeDraw.Layers(layers), card.draw(0))
    }

    // A failed node is not tried and reported again each time the product redraws the same material.
    @Test
    fun `a dropped node stays plain for the same layers and draws again for new ones`() {
        val slot = CardMaterialsSlot()
        nodes(NodeMaterials.Ready(layers))
        val card = slot.of(resolution, guard, allPlain)
        card.draw(0)
        card.failed(0, tint, MaterialFailure.UNSUPPORTED)

        val sameAgain = ResolvedMaterials(listOf(NodeMaterials.Ready(listOf(ReadyLayer(tint.copyOf(), shader(), emptyList())))), 7u)
        slot.of(mockk(relaxed = true) { every { resolved } returns MutableStateFlow(sameAgain) }, guard, allPlain)
        val sameDrawn = card.draw(0)

        val other = listOf(ReadyLayer(ByteArray(32) { 2 }, shader(), emptyList()))
        slot.of(mockk(relaxed = true) { every { resolved } returns MutableStateFlow(ResolvedMaterials(listOf(NodeMaterials.Ready(other)), 7u)) }, guard, allPlain)

        assertEquals(listOf(NodeDraw.Plain, NodeDraw.Layers(other)), listOf(sameDrawn, card.draw(0)))
    }

    // The core sends fresh byte arrays on every update; a node whose layers did not change must not
    // restart its surface, reload its images or be timed again.
    @Test
    fun `layers with the same content are equal`() {
        val copy = listOf(ReadyLayer(tint.copyOf(), shader(), emptyList()))

        assertEquals(NodeDraw.Layers(layers), NodeDraw.Layers(copy))
        assertEquals(NodeDraw.Layers(layers).hashCode(), NodeDraw.Layers(copy).hashCode())
        assertNotEquals(NodeDraw.Layers(layers), NodeDraw.Layers(listOf(ReadyLayer(ByteArray(32) { 2 }, shader(), emptyList()))))
    }

    // With the switch off, or nothing to draw, the card must not start GL or snapshot itself.
    @Test
    fun `a card takes the materials path only while a node can draw layers`() {
        val slot = CardMaterialsSlot()
        val plain = ResolvedMaterials(listOf(NodeMaterials.Plain(MaterialFailure.UNSUPPORTED)), 7u)
        val loading = ResolvedMaterials(listOf(NodeMaterials.Plain(MaterialFailure.UNSUPPORTED), NodeMaterials.Loading), 7u)

        val taken = listOf(null, plain, loading, null, plain).map { slot.drawsMaterials(it) }

        assertEquals(listOf(false, false, true, true, false), taken)
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
