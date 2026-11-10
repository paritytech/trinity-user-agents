package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DockletDrawbridgeTest {
    private val reference = drawbridge(402.dp, 874.dp)

    @Test
    fun `the pivot sits 14 dp past the dock's right end at its vertical centre`() {
        assertEquals(DockletPoint(406f, 816f), reference.pivot)
    }

    @Test
    fun `a shown dock is not turned or moved`() {
        assertEquals(DockletPose.Zero, reference.plainPose(0f))
    }

    @Test
    fun `half way the dock has turned 45 degrees and slid an eighth of the hidden slide`() {
        val pose = reference.plainPose(0.5f)

        assertEquals(45f, pose.angleDegrees, EPS)
        assertEquals(70f / 8f, pose.dx, EPS)
    }

    @Test
    fun `hidden, the dock stands on end past the right edge from y 420 to 802`() {
        val pose = reference.plainPose(1f)
        val leftEnd = reference.map(DockletPoint(10f, 816f), pose)
        val rightEnd = reference.map(DockletPoint(392f, 816f), pose)

        assertEquals(90f, pose.angleDegrees, EPS)
        assertEquals(420f, leftEnd.y, EPS)
        assertEquals(802f, rightEnd.y, EPS)
        assertTrue(leftEnd.x - DockletGeometry.DockHeight.value / 2 > 402f)
    }

    @Test
    fun `map and unmap are inverse`() {
        val pose = DockletPose(0.7f, 12f, -5f)
        val point = DockletPoint(120f, 800f)

        val back = reference.unmap(reference.map(point, pose), pose)

        assertEquals(point.x, back.x, EPS)
        assertEquals(point.y, back.y, EPS)
    }

    @Test
    fun `hide travel is three quarters of the dock from the first slot`() {
        assertEquals(286.5f, reference.hideTravel(touchX = 50f), EPS)
    }

    @Test
    fun `hide travel is cut to 1_2 times the room to the edge near the last slot, at least 60 dp`() {
        assertEquals(68.4f, reference.hideTravel(touchX = 345f), EPS)
        assertEquals(60f, reference.hideTravel(touchX = 395f), EPS)
    }

    @Test
    fun `progress past either end rubber-bands and never passes a quarter`() {
        assertEquals(0.5f, DockletDrawbridge.progressFromTravel(0.5f), EPS)
        assertEquals(1f + 0.25f * (1f - 1f / 2.1f), DockletDrawbridge.progressFromTravel(1.5f), EPS)
        assertTrue(DockletDrawbridge.progressFromTravel(100f) < 1.25f)
        assertTrue(DockletDrawbridge.progressFromTravel(-100f) > -0.25f)
    }

    @Test
    fun `150 dp the wrong way moves the dock about 31 dp, at most 40`() {
        assertEquals(31.6f, DockletDrawbridge.rubberBand(150f, 40f), 0.1f)
        assertTrue(DockletDrawbridge.rubberBand(10_000f, 40f) < 40f)
    }

    @Test
    fun `the edge glow runs 200 dp down from the hidden dock's top, 24 dp wide`() {
        assertEquals(DockletFrame(378.dp, 420.dp, 24.dp, 200.dp), reference.glowFrame)
        assertEquals(DockletFrame(355.dp, 420.dp, 47.dp, 200.dp), reference.grabZone)
    }

    @Test
    fun `the glow keeps 56 dp below the top and 8 dp above the bottom on a short screen`() {
        val short = drawbridge(402.dp, 250.dp)

        assertEquals(56f, short.glowFrame.top.value, EPS)
        assertEquals(250f - 8f - 56f, short.glowFrame.height.value, EPS)
    }

    @Test
    fun `the rules follow the dock on other screens`() {
        listOf(360.dp to 800.dp, 411.dp to 891.dp).forEach { (width, height) ->
            val bridge = drawbridge(width, height)
            val dock = dockFrame(width, height)
            val hidden = bridge.plainPose(1f)

            assertEquals(dock.right.value + 14f, bridge.pivot.x, EPS)
            assertEquals(width.value - dock.right.value + 60f, hidden.dx, EPS)
            assertEquals(0.75f * dock.width.value, bridge.hideTravel(touchX = dock.left.value), EPS)
            assertEquals(bridge.map(DockletPoint(dock.left.value, bridge.pivot.y), DockletPose(hidden.angle, 0f, 0f)).y, bridge.glowFrame.top.value, EPS)
        }
    }

    @Test
    fun `the Back pull band runs from the glow's top to the dock's old bottom, 24 dp either way`() {
        assertEquals(396f..871f, reference.backBand())
    }

    @Test
    fun `a finger low and far from the edge aims the dock fully out, high at the edge fully in`() {
        assertEquals(0f, reference.aimedProgress(DockletPoint(100f, 860f), 1f), EPS)
        assertTrue(reference.aimedProgress(DockletPoint(401f, 100f), 1f) > 0.9f)
    }

    @Test
    fun `a pull starts from the hidden pose and blends into the aim over 80 dp`() {
        val aim = DockletAim(reference, DockletPoint(395f, 500f), 1f, reference.plainPose(1f))

        aim.track(DockletPoint(394f, 500f))
        val nearStart = aim.progress
        aim.track(DockletPoint(395f - 134f, 760f))

        assertTrue(nearStart > 0.95f)
        assertEquals(reference.aimedProgress(DockletPoint(261f, 760f), aim.progress), aim.progress, 0.02f)
    }

    @Test
    fun `a held dock turns half the angle and keeps the grabbed point under the finger`() {
        val touch = DockletPoint(200f, 816f)
        val hold = DockletHold(reference, touch, 0f, reference.hideTravel(touch.x), DockletPose.Zero)
        val finger = DockletPoint(250f, 780f)

        hold.track(finger, 0.4f)
        val pose = hold.pose()

        assertEquals(0.5f * 0.4f * 90f, pose.angleDegrees, EPS)
        val grabbed = reference.map(touch, pose)
        val lead = 56f * DockletDrawbridge.smoothstep(0.4f)
        assertEquals(finger.x + lead, grabbed.x, EPS)
        assertEquals(finger.y, grabbed.y, EPS)
    }

    @Test
    fun `the edge cue keeps 14 dp of dock behind the finger`() {
        val touch = DockletPoint(30f, 816f)
        val hold = DockletHold(reference, touch, 0f, reference.hideTravel(touch.x), DockletPose.Zero)

        hold.track(DockletPoint(200f, 816f), 1f)

        assertEquals(200f + (30f - 10f - 14f), reference.map(touch, hold.pose()).x, EPS)
    }

    @Test
    fun `a touch off the dock outline turns the dock about the pivot`() {
        val touch = DockletPoint(200f, 700f)
        val hold = DockletHold(reference, touch, 0f, reference.hideTravel(touch.x), DockletPose.Zero)

        hold.track(DockletPoint(300f, 700f), 0.3f)

        assertEquals(reference.plainPose(0.3f), hold.pose())
    }

    @Test
    fun `a drag below the screen gives at most 36 dp and the dock never sinks past it`() {
        val touch = DockletPoint(200f, 816f)
        val hold = DockletHold(reference, touch, 0f, reference.hideTravel(touch.x), DockletPose.Zero)

        hold.track(DockletPoint(220f, 2000f), 0.1f)

        assertTrue(reference.lowestY(hold.pose()) <= 874f + 36f + EPS)
    }

    @Test
    fun `above the glow top the dock follows at most 40 dp more`() {
        val touch = DockletPoint(200f, 816f)
        val hold = DockletHold(reference, touch, 0f, reference.hideTravel(touch.x), DockletPose.Zero)

        hold.track(DockletPoint(250f, -1000f), 0.5f)

        assertTrue(reference.map(touch, hold.pose()).y >= 420f - 40f)
    }

    @Test
    fun `the peek dips 0_14 at its middle and is back at both ends`() {
        assertEquals(0f, DockletDrawbridge.peekDip(0f), EPS)
        assertEquals(0.14f, DockletDrawbridge.peekDip(0.5f), EPS)
        assertEquals(0f, DockletDrawbridge.peekDip(1f), EPS)
    }

    @Test
    fun `the quiet fade is gone before the dock lands`() {
        assertEquals(1f, DockletDrawbridge.quietFadeAlpha(0f), EPS)
        assertEquals(0f, DockletDrawbridge.quietFadeAlpha(0.95f), EPS)
        assertTrue(DockletDrawbridge.quietFadeAlpha(0.5f) in 0.4f..0.6f)
    }

    private fun dockFrame(width: Dp, height: Dp) = DockletGeometry.dockFrame(width, height, NavigationInset, threeButtonNavigation = false)

    private fun drawbridge(width: Dp, height: Dp) = DockletDrawbridge(dockFrame(width, height), width, height)

    private companion object {
        const val EPS = 1e-3f
        val NavigationInset = 34.dp
    }
}
