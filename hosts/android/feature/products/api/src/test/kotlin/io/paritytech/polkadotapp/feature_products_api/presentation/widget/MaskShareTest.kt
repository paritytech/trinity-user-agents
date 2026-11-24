package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.MaskShareWatch.Input
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The same cases as the core's `cleared_share` tests: a 10 by 6 dp node is 5 by 3 samples. */
class MaskShareTest {
    private val width = 1000
    private val height = 600

    private fun count(vararg shapes: JsMaskShape) = MaskCoverage.count(shapes.toList(), width, height)

    @Test
    fun `the cleared share samples two dp squares from the top-start corner`() {
        assertEquals(0 to 15, count())
        assertEquals(15 to 15, count(JsMaskShape.All))
        // A part square counts as a whole sample: 1001 wide is 6 columns.
        assertEquals(0 to 18, MaskCoverage.count(emptyList(), 1001, 600))
        assertEquals(0 to 0, MaskCoverage.count(emptyList(), 0, 600))
    }

    @Test
    fun `a shape covers the samples on its edge and not one past it`() {
        fun circle(radius: Long) = JsMaskShape.Circle(JsMaskPoint(100, 100), radius)
        assertEquals(3 to 15, count(circle(200)))
        assertEquals(1 to 15, count(circle(199)))
        // A stroke along the middle row, 400 wide: its edges pass through the rows above and below.
        fun row(stroke: Long) = JsMaskShape.Stroke(stroke, listOf(JsMaskPoint(100, 300), JsMaskPoint(900, 300)))
        assertEquals(15 to 15, count(row(400)))
        assertEquals(5 to 15, count(row(398)))
        // Round caps: a dot is a circle half its width across.
        assertEquals(5 to 15, count(JsMaskShape.Stroke(400, listOf(JsMaskPoint(500, 300)))))
        // A shape reaching outside the node counts only the samples inside it.
        assertEquals(1 to 15, count(JsMaskShape.Circle(JsMaskPoint(-100, -100), 300)))
        // Overlapping shapes count a sample once.
        assertEquals(3 to 15, count(circle(200), circle(200)))
    }

    @Test
    fun `a cleared share is reached exactly at its line`() {
        fun share(atPermille: Int) = JsClearedShare(atPermille, "a")
        assertTrue(share(550).reached(713, 1295))
        assertFalse(share(550).reached(712, 1295))
        assertTrue(share(500).reached(2, 4))
        assertFalse(share(500).reached(1, 4))
        assertTrue(share(0).reached(0, 4))
        assertFalse(share(1001).reached(4, 4))
        assertFalse(share(UShort.MAX_VALUE.toInt()).reached(Int.MAX_VALUE, Int.MAX_VALUE))
    }

    @Test
    fun `a followed mask checks on the lift and on shapes added before the next touch, once per lift`() {
        val watch = MaskShareWatch()
        var reached = true
        fun update(input: Input) = watch.update(input, follows = true) { reached }

        assertFalse("never while the finger is down", update(Input(down = true)))
        assertFalse("nor for shapes added while it is down", update(Input(down = true, additions = 1)))
        assertTrue("the lift checks", update(Input(lifts = 1, additions = 1)))
        assertFalse("at most once per lift", update(Input(lifts = 1, additions = 2)))

        reached = false
        assertFalse(update(Input(down = true, lifts = 1, additions = 2)))
        assertFalse("a lift short of the share sends nothing", update(Input(lifts = 2, additions = 2)))
        reached = true
        assertTrue("the worker's answer to the end still counts", update(Input(lifts = 2, additions = 3)))
    }

    @Test
    fun `a new touch before the answer starts over`() {
        val watch = MaskShareWatch()
        fun update(input: Input, reached: Boolean) = watch.update(input, follows = true) { reached }

        assertFalse(update(Input(lifts = 1), reached = false))
        assertFalse(update(Input(down = true, lifts = 1), reached = true))
        assertFalse("shapes added after the new touch's lift wait for its own lift", update(Input(down = true, lifts = 1, additions = 1), reached = true))
    }

    @Test
    fun `a mask with no pan checks when shapes are added, once per tree, never for a tree's own shapes`() {
        val watch = MaskShareWatch()
        fun update(input: Input) = watch.update(input, follows = false) { true }

        assertFalse("a tree alone", update(Input(trees = 1)))
        assertTrue(update(Input(trees = 1, additions = 1)))
        assertFalse("once per tree", update(Input(trees = 1, additions = 2)))
        assertFalse(update(Input(trees = 2, additions = 2)))
        assertTrue("a new tree may send again", update(Input(trees = 2, additions = 3)))
    }

    @Test
    fun `what happened before the mask was drawn sends nothing`() {
        val watch = MaskShareWatch(Input(lifts = 4, additions = 9, trees = 2))

        assertFalse(watch.update(Input(lifts = 4, additions = 9, trees = 2), follows = true) { true })
    }

    @Test
    fun `a pan counts its lifts and a mask counts its additions and trees`() {
        val trail = JsPanTrail()
        trail.touch(isDown = true)
        trail.touch(isDown = true)
        trail.touch(isDown = false)
        trail.touch(isDown = false)
        assertFalse(trail.down)
        assertEquals(1, trail.lifts)

        val shapes = JsMaskShapes()
        shapes.extend(emptyList())
        assertEquals("an empty answer adds nothing", 0, shapes.additions)
        shapes.extend(listOf(JsMaskShape.All))
        shapes.treeArrived(next = null)
        assertEquals(1, shapes.additions)
        assertEquals(1, shapes.trees)
    }
}
