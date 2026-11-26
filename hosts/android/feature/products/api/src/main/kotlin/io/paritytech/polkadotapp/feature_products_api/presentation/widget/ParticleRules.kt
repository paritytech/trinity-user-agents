package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsBurst
import io.paritytech.polkadotapp.feature_products_api.model.JsBurstTrigger
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitSource
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitter
import io.paritytech.polkadotapp.feature_products_api.model.JsParticleLook
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.sin
import kotlin.random.Random

/**
 * A place in the node's frame, in hundredths of a dp: [x] from its start edge along the layout direction, [y] down from
 * its top edge. Particles move in this frame, so an emitter's angles and places read the same in either direction.
 */
internal data class ParticlePoint(val x: Double, val y: Double)

/** One particle: what it looks like, where and when it left, and the emitter whose rules move it. */
internal class Particle(
    val emitter: JsEmitter,
    val look: JsParticleLook,
    val start: ParticlePoint,
    val speed: Double,
    val direction: Double,
    val spin: Double,
    val bornNanos: Long,
)

/** Where a particle is drawn at one moment: its place, its opacity and scale as shares, and its turn in degrees. */
internal data class ParticlePlace(val x: Double, val y: Double, val opacity: Double, val scale: Double, val rotation: Double)

internal object ParticleRules {
    /** The most particles alive at once from one emitter, whatever it asks for. */
    const val MAX_ALIVE = 512

    /**
     * Where a particle is [ageMs] after it left [start] at [speed] in [direction] (radians, clockwise from the end
     * direction), or null once its life is over: the core's `Emitter::place_at`.
     */
    fun placeAt(emitter: JsEmitter, start: ParticlePoint, speed: Double, direction: Double, ageMs: Long): ParticlePlace? {
        if (ageMs >= emitter.lifeMs) return null
        val seconds = ageMs / MS_PER_S
        val life = ageMs.toDouble() / emitter.lifeMs
        val x = start.x + speed * cos(direction) * seconds
        val y = start.y + speed * sin(direction) * seconds + emitter.gravity * seconds * seconds / 2
        val fadeFrom = (emitter.fadeFromPermille / PERMILLE).coerceAtMost(1.0)
        val opacity = if (life <= fadeFrom) 1.0 else (1.0 - life) / (1.0 - fadeFrom)
        val endScale = emitter.endScalePermille / PERMILLE
        return ParticlePlace(x, y, opacity, 1.0 + (endScale - 1.0) * life, 0.0)
    }

    /** [placeAt] for a [particle] at [nowNanos], with its turn. */
    fun place(particle: Particle, nowNanos: Long): ParticlePlace? {
        val ageMs = ((nowNanos - particle.bornNanos).coerceAtLeast(0L)) / NANOS_PER_MS
        val place = placeAt(particle.emitter, particle.start, particle.speed, particle.direction, ageMs) ?: return null
        return place.copy(rotation = particle.spin / HUNDREDTHS * ageMs / MS_PER_S)
    }

    /** The direction for an even random [share], spread across the full width around the middle, in radians. */
    fun direction(emitter: JsEmitter, share: Double): Double =
        Math.toRadians((emitter.angleCentideg + (share.coerceIn(0.0, 1.0) - HALF) * emitter.spreadCentideg) / HUNDREDTHS)

    fun cap(emitter: JsEmitter): Int = emitter.maxAlive.coerceIn(0, MAX_ALIVE)

    /**
     * The bursts a tree sets off by changing a value: each `Change` trigger whose value differs from the one at the
     * same place among the previous tree's. The first tree with the emitter ([previous] null) sets none off.
     */
    fun changed(previous: List<JsBurst>?, current: List<JsBurst>): List<JsBurst> {
        previous ?: return emptyList()
        val before = previous.filter { it.on is JsBurstTrigger.Change }
        return current.filter { it.on is JsBurstTrigger.Change }.filterIndexed { index, burst ->
            val old = before.getOrNull(index)
            old != null && old.on != burst.on
        }
    }
}

/**
 * The particles one node sends and keeps: emitted on the frame clock from its source at its rates, set off in bursts,
 * capped, and dropped at the end of their lives. Kept apart from drawing so it can be checked on its own.
 */
internal class ParticleSystem(private val random: Random = Random.Default) {
    val particles = ArrayList<Particle>()
    private var lastFrame: Long? = null
    private var timeCarry = 0.0
    private var distanceCarry = 0.0
    private var finger: ParticlePoint? = null

    /** Where the finger last was on a pan source, kept after it lifts, so a burst comes from there. */
    var lastFinger: ParticlePoint? = null
        private set

    val isEmpty: Boolean get() = particles.isEmpty()

    /**
     * Moves the clock to [now]: drops particles whose life is over and, from a live [emitter], sends what its rates owe
     * since the last frame in a node [width] by [height] (hundredths of a dp). For a pan source [fingerAt] is where the
     * finger is now, or null when it is up.
     */
    fun step(now: Long, emitter: JsEmitter?, width: Double, height: Double, fingerAt: ParticlePoint?) {
        particles.removeAll { ParticleRules.place(it, now) == null }
        val elapsedS = lastFrame?.let { (now - it).coerceAtLeast(0L) / NANOS_PER_S } ?: 0.0
        lastFrame = now
        if (emitter == null) {
            timeCarry = 0.0
            distanceCarry = 0.0
            finger = null
            return
        }
        when (emitter.source) {
            is JsEmitSource.Pan -> {
                val before = finger
                finger = fingerAt
                if (fingerAt == null) {
                    timeCarry = 0.0
                    distanceCarry = 0.0
                    return
                }
                lastFinger = fingerAt
                repeat(owed(emitter.ratePerS * elapsedS, time = true)) { send(emitter, fingerAt, now) }
                if (before != null) {
                    val moved = hypot(fingerAt.x - before.x, fingerAt.y - before.y) / HUNDREDTHS
                    val count = owed(emitter.ratePer100pt * moved / HUNDREDTHS, time = false)
                    // Spaced evenly along the way the finger went since the last frame, the last one where it is now.
                    for (index in 1..count) {
                        val share = index.toDouble() / count
                        send(emitter, ParticlePoint(before.x + (fingerAt.x - before.x) * share, before.y + (fingerAt.y - before.y) * share), now)
                    }
                }
            }
            else -> repeat(owed(emitter.ratePerS * elapsedS, time = true)) { send(emitter, origin(emitter, width, height), now) }
        }
    }

    /**
     * Sends [count] particles at once from the source, at most the most a host keeps alive; a pan source sends them
     * from where the finger last was.
     */
    fun burst(emitter: JsEmitter, count: Int, now: Long, width: Double, height: Double) {
        val pan = emitter.source is JsEmitSource.Pan
        val from = if (pan) lastFinger ?: return else null
        repeat(count.coerceIn(0, ParticleRules.MAX_ALIVE)) { send(emitter, from ?: origin(emitter, width, height), now) }
    }

    private fun owed(amount: Double, time: Boolean): Int {
        val total = (if (time) timeCarry else distanceCarry) + amount
        val whole = floor(total)
        val left = total - whole
        if (time) timeCarry = left else distanceCarry = left
        // A long pause owes no more than could ever be alive.
        return whole.toInt().coerceAtMost(ParticleRules.MAX_ALIVE)
    }

    private fun origin(emitter: JsEmitter, width: Double, height: Double): ParticlePoint = when (val source = emitter.source) {
        is JsEmitSource.Point -> ParticlePoint(source.x.toDouble(), source.y.toDouble())
        is JsEmitSource.Area -> ParticlePoint(random.nextDouble() * width, random.nextDouble() * height)
        is JsEmitSource.Pan -> lastFinger ?: ParticlePoint(0.0, 0.0)
    }

    // Particles past the cap are not sent at all.
    private fun send(emitter: JsEmitter, at: ParticlePoint, now: Long) {
        if (emitter.looks.isEmpty() || particles.size >= ParticleRules.cap(emitter)) return
        particles += Particle(
            emitter = emitter,
            look = emitter.looks[random.nextInt(emitter.looks.size)],
            start = at,
            speed = emitter.speed.pick(random.nextDouble()),
            direction = ParticleRules.direction(emitter, random.nextDouble()),
            spin = emitter.spin.pick(random.nextDouble()),
            bornNanos = now,
        )
    }
}

private const val MS_PER_S = 1000.0
private const val NANOS_PER_MS = 1_000_000L
private const val NANOS_PER_S = 1_000_000_000.0
private const val PERMILLE = 1000.0
private const val HUNDREDTHS = 100.0
private const val HALF = 0.5
