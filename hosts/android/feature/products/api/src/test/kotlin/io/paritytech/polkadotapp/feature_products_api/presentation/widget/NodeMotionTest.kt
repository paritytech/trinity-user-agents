package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsAnimatedProperty
import io.paritytech.polkadotapp.feature_products_api.model.JsCurve
import io.paritytech.polkadotapp.feature_products_api.model.JsEnter
import io.paritytech.polkadotapp.feature_products_api.model.JsKeyframe
import io.paritytech.polkadotapp.feature_products_api.model.JsMotion
import io.paritytech.polkadotapp.feature_products_api.model.JsPose
import io.paritytech.polkadotapp.feature_products_api.model.JsTrack
import io.paritytech.polkadotapp.feature_products_api.model.JsTrackPlay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NodeMotionTest {
    private val linear = JsCurve(0, 0, 1000, 1000)
    private val timed = JsMotion.Timed(1000, linear)
    private val spring = JsMotion.Spring(1800, 80)

    @Test
    fun `curves clamp time and horizontal controls while allowing overshoot`() {
        assertEquals(0.25, NodeMotionRules.ease(linear, 0.25), 0.000001)
        assertEquals(0.0, NodeMotionRules.ease(linear, -1.0), 0.000001)
        assertEquals(1.0, NodeMotionRules.ease(linear, 2.0), 0.000001)
        assertEquals(NodeMotionRules.ease(linear, 0.25), NodeMotionRules.ease(JsCurve(-500, 0, 1500, 1000), 0.25), 0.000001)
        assertTrue(NodeMotionRules.ease(JsCurve(0, 2000, 1000, 2000), 0.5) > 1)
    }

    @Test
    fun `timeline times never decrease and the last frame at a shared time holds`() {
        val timeline = NodeTimeline(frames(listOf(0L to 0, 100L to 100, 50L to 200, 200L to 400)))
        assertEquals(50.0, requireNotNull(timeline.sample(50, true).value), 0.000001)
        assertEquals(200.0, requireNotNull(timeline.sample(100, true).value), 0.000001)
        assertEquals(300.0, requireNotNull(timeline.sample(150, true).value), 0.000001)
        assertEquals(400.0, requireNotNull(timeline.sample(200, true).value), 0.000001)
    }

    @Test
    fun `alternating finite runs hold the end of their final direction`() {
        val timeline = NodeTimeline(frames(listOf(0L to 0, 100L to 100), repeat = 2, alternate = true))
        assertEquals(50.0, requireNotNull(timeline.sample(150, true).value), 0.000001)
        assertEquals(TimelineSample(0.0, true), timeline.sample(500, true))
        assertEquals(TimelineSample(0.0, true), timeline.sample(0, false))
    }

    @Test
    fun `infinite timelines hold their first frame with less motion and do not finish normally`() {
        val timeline = NodeTimeline(frames(listOf(0L to 200, 100L to 1000), repeat = 0))
        assertEquals(TimelineSample(200.0, true), timeline.sample(500, false))
        assertFalse(timeline.sample(1_000_050, true).finished)
    }

    @Test
    fun `empty zero duration and oversized timelines have bounded sampling`() {
        assertEquals(TimelineSample(null, true), NodeTimeline(frames(emptyList())).sample(0, true))
        assertEquals(TimelineSample(200.0, true), NodeTimeline(frames(listOf(0L to 0, 0L to 200))).sample(0, true))
        val timeline = NodeTimeline(frames((0..40).map { it * 100L to it * 10 }))
        assertEquals(TimelineSample(310.0, true), timeline.sample(10000, true))
    }

    @Test
    fun `timed motion has a defined velocity and zero duration lands immediately`() {
        assertEquals(50.0, NodeMotionRules.motion(timed, 0.0, 100.0, 0.0, 500_000_000).value, 0.000001)
        assertEquals(100.0, NodeMotionRules.motion(timed, 0.0, 100.0, 0.0, 500_000_000).velocity, 0.000001)
        assertEquals(MotionSample(100.0, 0.0, true), NodeMotionRules.motion(timed.copy(ms = 0), 0.0, 100.0, 0.0, 0))
    }

    @Test
    fun `springs start with the supplied velocity and all damped regimes settle`() {
        for (damping in listOf(80, 100, 200)) {
            val motion = spring.copy(dampingCenti = damping)
            val first = NodeMotionRules.motion(motion, 50.0, 100.0, 30.0, 0)
            assertEquals(50.0, first.value, 0.000001)
            assertEquals(30.0, first.velocity, 0.000001)
            assertEquals(MotionSample(100.0, 0.0, true), NodeMotionRules.motion(motion, 50.0, 100.0, 30.0, 5_000_000_000))
        }
        assertFalse(NodeMotionRules.motion(spring.copy(dampingCenti = 0), 0.0, 100.0, 0.0, 10_000_000_000).finished)
    }

    @Test
    fun `enter holds its delay and keeps its clock across new trees`() {
        val player = NodeMotionPlayer(JsEnter(JsPose(0, 1000, 1000, 0, 0), timed, 100), emptyList())
        assertEquals(0f, player.initialPose(true).opacity, 0f)
        player.sample(0, true)
        assertEquals(0f, player.sample(50_000_000, true).pose.opacity, 0f)
        assertEquals(0.5f, player.sample(600_000_000, true).pose.opacity, 0.00001f)
        player.update(emptyList())
        val later = player.sample(850_000_000, true)
        assertEquals(0.75f, later.pose.opacity, 0.00001f)
        assertEquals(2.5f, later.pose.x, 0.00001f)
        assertFalse(player.sample(1_100_000_000, true).running)
    }

    @Test
    fun `unchanged timelines keep their clock and changed timelines restart`() {
        val timeline = frames(listOf(0L to 0, 1000L to 1000))
        val track = JsTrack(JsAnimatedProperty.OPACITY, timeline)
        val player = NodeMotionPlayer(null, listOf(track))
        player.sample(0, true)
        assertEquals(0.4f, player.sample(400_000_000, true).pose.opacity, 0.00001f)
        player.update(listOf(track.copy(play = timeline.copy())))
        assertEquals(0.6f, player.sample(600_000_000, true).pose.opacity, 0.00001f)
        player.update(listOf(track.copy(play = frames(listOf(0L to 0, 2000L to 1000)))))
        assertEquals(0f, player.sample(800_000_000, true).pose.opacity, 0.00001f)
        assertEquals(0.5f, player.sample(1_800_000_000, true).pose.opacity, 0.00001f)
    }

    @Test
    fun `the last track per property composes with enter and removing a track restores its neutral value`() {
        val enter = JsEnter(JsPose(500, 800, 1000, -500, 4500), timed, 100)
        val player = NodeMotionPlayer(enter, listOf(follow(JsAnimatedProperty.OPACITY, 200), follow(JsAnimatedProperty.OPACITY, 800), follow(JsAnimatedProperty.OFFSET_X, 500), follow(JsAnimatedProperty.SCALE, 1500), follow(JsAnimatedProperty.ROTATION, 4500)))
        val first = player.sample(0, true).pose
        assertEquals(0.4f, first.opacity, 0.00001f)
        assertEquals(15f, first.x, 0.00001f)
        assertEquals(-5f, first.y, 0.00001f)
        assertEquals(1.2f, first.scale, 0.00001f)
        assertEquals(90f, first.rotation, 0.00001f)
        player.update(emptyList())
        assertEquals(0.5f, player.sample(0, true).pose.opacity, 0.00001f)
    }

    @Test
    fun `a following spring retargets from the drawn value and carries its velocity`() {
        val player = NodeMotionPlayer(null, listOf(follow(JsAnimatedProperty.OFFSET_X, 0, spring)))
        player.sample(0, true)
        player.update(listOf(follow(JsAnimatedProperty.OFFSET_X, 10000, spring)))
        player.sample(16_000_000, true)
        val before = player.sample(116_000_000, true).pose.x
        val physical = NodeMotionRules.motion(spring, 0.0, 10000.0, 0.0, 100_000_000)
        player.update(listOf(follow(JsAnimatedProperty.OFFSET_X, -5000, spring)))
        assertEquals(before, player.sample(132_000_000, true).pose.x, 0.00001f)
        val expected = NodeMotionRules.motion(spring, physical.value, -5000.0, physical.velocity, 100_000_000)
        assertEquals((expected.value / 100).toFloat(), player.sample(232_000_000, true).pose.x, 0.0001f)
    }

    @Test
    fun `a following target after an idle period begins at the next frame rather than the old frame time`() {
        val player = NodeMotionPlayer(null, listOf(follow(JsAnimatedProperty.OFFSET_X, 0)))
        player.sample(0, true)
        player.update(listOf(follow(JsAnimatedProperty.OFFSET_X, 10000)))
        assertEquals(0f, player.sample(10_000_000_000, true).pose.x, 0.00001f)
        assertEquals(50f, player.sample(10_500_000_000, true).pose.x, 0.00001f)
    }

    @Test
    fun `a tree preview removes tracks immediately without starting a pending following clock`() {
        val player = NodeMotionPlayer(null, listOf(follow(JsAnimatedProperty.OPACITY, 200), follow(JsAnimatedProperty.OFFSET_X, 0)))
        player.sample(0, true)
        player.update(listOf(follow(JsAnimatedProperty.OFFSET_X, 10000)))
        assertEquals(1f, player.preview(true).opacity, 0f)
        assertEquals(0f, player.preview(true).x, 0f)
        assertEquals(0f, player.sample(10_000_000_000, true).pose.x, 0.00001f)
        assertEquals(50f, player.sample(10_500_000_000, true).pose.x, 0.00001f)
    }

    @Test
    fun `less motion skips enter and snaps following targets`() {
        val player = NodeMotionPlayer(JsEnter(JsPose(0, 0, 10000, 10000, 9000), timed, 100), listOf(follow(JsAnimatedProperty.OFFSET_X, 300)))
        assertEquals(NodePose(1f, 1f, 3f, 0f, 0f), player.initialPose(false))
        player.sample(0, false)
        player.update(listOf(follow(JsAnimatedProperty.OFFSET_X, 800)))
        assertEquals(NodePose(1f, 1f, 8f, 0f, 0f), player.sample(100_000_000, false).pose)
        assertFalse(player.sample(200_000_000, true).running)
    }

    private fun frames(values: List<Pair<Long, Int>>, repeat: Int = 1, alternate: Boolean = false) =
        JsTrackPlay.Keyframes(values.map { JsKeyframe(it.first, it.second, linear) }, repeat, alternate)

    private fun follow(property: JsAnimatedProperty, target: Int, motion: JsMotion = timed) =
        JsTrack(property, JsTrackPlay.Follow(target, motion))
}
