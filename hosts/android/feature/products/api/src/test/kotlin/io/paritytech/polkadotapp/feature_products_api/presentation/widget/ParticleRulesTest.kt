package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsBurst
import io.paritytech.polkadotapp.feature_products_api.model.JsBurstTrigger
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitSource
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitter
import io.paritytech.polkadotapp.feature_products_api.model.JsParticleLook
import io.paritytech.polkadotapp.feature_products_api.model.JsValueRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.random.Random

class ParticleRulesTest {
    private val dot = JsParticleLook.Circle(400, 0xFF0000FF)

    private fun emitter(
        source: JsEmitSource = JsEmitSource.Point(1000, 2000),
        ratePerS: Int = 0,
        ratePer100pt: Int = 0,
        lifeMs: Int = 1000,
        speed: JsValueRange = JsValueRange(0, 0),
        gravity: Int = 0,
        fadeFromPermille: Int = 1000,
        endScalePermille: Int = 1000,
        maxAlive: Int = 512,
        bursts: List<JsBurst> = emptyList(),
        looks: List<JsParticleLook> = listOf(dot),
    ) = JsEmitter(
        looks, source, ratePerS, ratePer100pt, lifeMs, speed, angleCentideg = 0, spreadCentideg = 0, gravity = gravity,
        spin = JsValueRange(0, 0), fadeFromPermille = fadeFromPermille, endScalePermille = endScalePermille,
        maxAlive = maxAlive, bursts = bursts,
    )

    @Test
    fun `a place follows the core rule for motion, gravity, fade and scale`() {
        val rules = emitter(lifeMs = 2000, gravity = 1000, fadeFromPermille = 500, endScalePermille = 0)
        val place = ParticleRules.placeAt(rules, ParticlePoint(100.0, 200.0), 400.0, PI / 2, 1500)!!
        assertEquals(100.0, place.x, 0.0001)
        // 200 + 400 * 1.5 + 1000 * 1.5² / 2
        assertEquals(1925.0, place.y, 0.0001)
        assertEquals(0.5, place.opacity, 0.0001)
        assertEquals(0.25, place.scale, 0.0001)
        assertNull(ParticleRules.placeAt(rules, ParticlePoint(0.0, 0.0), 0.0, 0.0, 2000))
    }

    @Test
    fun `a fade from at or past the whole life never fades`() {
        val place = ParticleRules.placeAt(emitter(fadeFromPermille = 1500), ParticlePoint(0.0, 0.0), 0.0, 0.0, 999)!!
        assertEquals(1.0, place.opacity, 0.0001)
    }

    @Test
    fun `ranges swap reversed ends and directions spread around the middle clockwise from the end`() {
        assertEquals(150.0, JsValueRange(200, 100).pick(0.5), 0.0001)
        assertEquals(100.0, JsValueRange(200, 100).pick(-1.0), 0.0001)
        val spread = emitter().copy(angleCentideg = 9000, spreadCentideg = 2000)
        assertEquals(PI / 2 - PI / 18, ParticleRules.direction(spread, 0.0), 0.0001)
        assertEquals(PI / 2 + PI / 18, ParticleRules.direction(spread, 1.0), 0.0001)
    }

    @Test
    fun `a spinning particle turns from upright at its speed`() {
        val rules = emitter().copy(spin = JsValueRange(9000, 9000))
        val system = ParticleSystem(Random(1))
        system.burst(rules, 1, 0L, 0.0, 0.0)
        assertEquals(45.0, ParticleRules.place(system.particles.single(), 500_000_000L)!!.rotation, 0.0001)
    }

    @Test
    fun `a point sends its rate a second on the frame clock and drops particles at the end of life`() {
        val rules = emitter(ratePerS = 10, lifeMs = 1000)
        val system = ParticleSystem(Random(1))
        system.step(0L, rules, 1000.0, 1000.0, null)
        system.step(250_000_000L, rules, 1000.0, 1000.0, null)
        assertEquals(2, system.particles.size)
        assertTrue(system.particles.all { it.start == ParticlePoint(1000.0, 2000.0) })
        system.step(1_000_000_000L, rules, 1000.0, 1000.0, null)
        assertEquals(10, system.particles.size)
        system.step(1_300_000_000L, null, 1000.0, 1000.0, null)
        // The two sent by 0.25 s are over; the eight sent at 1 s live on with no emitter.
        assertEquals(8, system.particles.size)
    }

    @Test
    fun `the area sends anywhere in the node`() {
        val system = ParticleSystem(Random(7))
        system.burst(emitter(source = JsEmitSource.Area), 200, 0L, 3000.0, 500.0)
        assertTrue(system.particles.all { it.start.x in 0.0..3000.0 && it.start.y in 0.0..500.0 })
        assertTrue(system.particles.any { it.start.x > 1500 })
    }

    @Test
    fun `at most the cap is alive, and the cap is never past 512`() {
        val system = ParticleSystem(Random(1))
        system.burst(emitter(maxAlive = 3), 10, 0L, 0.0, 0.0)
        assertEquals(3, system.particles.size)
        assertEquals(512, ParticleRules.cap(emitter(maxAlive = 65535)))
        val many = ParticleSystem(Random(1))
        many.burst(emitter(maxAlive = 65535), 1000, 0L, 0.0, 0.0)
        assertEquals(512, many.particles.size)
    }

    @Test(timeout = 1000)
    fun `a burst however large sends at most the most a host keeps alive`() {
        val system = ParticleSystem(Random(1))
        system.burst(emitter(maxAlive = 65535), Int.MAX_VALUE, 0L, 0.0, 0.0)
        assertEquals(ParticleRules.MAX_ALIVE, system.particles.size)
    }

    @Test
    fun `a pan sends along the way the finger went, only while it is down, and bursts where it last was`() {
        val rules = emitter(source = JsEmitSource.Pan("p"), ratePer100pt = 4)
        val system = ParticleSystem(Random(1))
        system.step(0L, rules, 0.0, 0.0, null)
        system.step(16L, rules, 0.0, 0.0, ParticlePoint(0.0, 0.0))
        assertEquals(0, system.particles.size)
        // 50 logical pixels owe two particles, at half way and at the finger.
        system.step(32L, rules, 0.0, 0.0, ParticlePoint(5000.0, 0.0))
        assertEquals(listOf(2500.0, 5000.0), system.particles.map { it.start.x })
        system.step(48L, rules, 0.0, 0.0, null)
        system.step(64L, rules, 0.0, 0.0, ParticlePoint(0.0, 0.0))
        assertEquals(2, system.particles.size)
        system.step(80L, rules, 0.0, 0.0, null)
        system.burst(rules, 1, 96L, 0.0, 0.0)
        assertEquals(ParticlePoint(0.0, 0.0), system.particles.last().start)
    }

    @Test
    fun `a pan that was never touched sends no burst`() {
        val system = ParticleSystem(Random(1))
        system.burst(emitter(source = JsEmitSource.Pan("p")), 5, 0L, 0.0, 0.0)
        assertTrue(system.isEmpty)
    }

    @Test
    fun `an emitter with no looks sends nothing`() {
        val system = ParticleSystem(Random(1))
        system.burst(emitter(looks = emptyList()), 5, 0L, 0.0, 0.0)
        assertTrue(system.isEmpty)
    }

    @Test
    fun `particles keep the rules of the emitter that sent them`() {
        val system = ParticleSystem(Random(1))
        system.burst(emitter(lifeMs = 5000), 1, 0L, 0.0, 0.0)
        system.step(2_000_000_000L, emitter(lifeMs = 1000), 0.0, 0.0, null)
        assertEquals(1, system.particles.size)
    }

    @Test
    fun `a changed value bursts, the first tree and unchanged values do not`() {
        fun bursts(vararg values: Long) = values.map { JsBurst(3, JsBurstTrigger.Change(it)) } + JsBurst(1, JsBurstTrigger.Appear)
        assertEquals(emptyList<JsBurst>(), ParticleRules.changed(null, bursts(1)))
        assertEquals(emptyList<JsBurst>(), ParticleRules.changed(bursts(1), bursts(1)))
        assertEquals(listOf(JsBurst(3, JsBurstTrigger.Change(4))), ParticleRules.changed(bursts(1, 2), bursts(1, 4)))
    }
}
