package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * The studio the coins mirror: softbox rectangles facing the origin and a room gradient behind them, a few
 * hundred bytes in `params.json`. It is baked into the prefiltered cube on the device ([CoinageStudioBake]);
 * this is the same evaluation on the CPU, which the bake falls back to and the tests check the GPU against.
 *
 * Cube face convention, shared with the bake shader and the coin shader's sampler: faces +X, −X, +Y, −Y, +Z,
 * −Z; texel (column i, row j) of an n×n face maps to s = 2(i+½)/n − 1, t = 2(j+½)/n − 1, row 0 first. Level L
 * of N is prefiltered for perceptual roughness L/(N−1), GGX with α = r², assuming N = V = R.
 */
class CoinageStudio(val lamps: List<Lamp>, val roomLow: Float, val roomHigh: Float) {
    /** One softbox as `params.json` carries it, already turned to face the origin. */
    class Lamp(
        val center: Vec,
        val uAxis: Vec,
        val vAxis: Vec,
        val normal: Vec,
        val halfU: Float,
        val halfV: Float,
        val tint: Vec,
        val intensity: Float,
        val roomHere: Float
    )

    data class Vec(val x: Float, val y: Float, val z: Float) {
        operator fun plus(o: Vec) = Vec(x + o.x, y + o.y, z + o.z)
        operator fun minus(o: Vec) = Vec(x - o.x, y - o.y, z - o.z)
        operator fun times(k: Float) = Vec(x * k, y * k, z * k)
        infix fun dot(o: Vec) = x * o.x + y * o.y + z * o.z
        infix fun cross(o: Vec) = Vec(y * o.z - z * o.y, z * o.x - x * o.z, x * o.y - y * o.x)
        fun normalized(): Vec = this * (1f / maxOf(sqrt(this dot this), 1e-9f))
    }

    /** Radiance arriving at the origin from unit direction [d]: the nearest lamp met from the front, else the room. */
    fun radiance(d: Vec): Vec {
        var best = Float.POSITIVE_INFINITY
        var hit: Lamp? = null
        var hitU = 0f
        var hitV = 0f

        for (lamp in lamps) {
            val denom = d dot lamp.normal

            if (denom >= -1e-6f) continue

            val t = (lamp.center dot lamp.normal) / denom

            if (t <= 0f || t >= best) continue

            val p = d * t - lamp.center
            val lu = (p dot lamp.uAxis) / lamp.halfU
            val lv = (p dot lamp.vAxis) / lamp.halfV

            if (abs(lu) > 1f || abs(lv) > 1f) continue

            best = t
            hit = lamp
            hitU = lu
            hitV = lv
        }

        if (hit != null) {
            val falloff = hit.intensity * (1f - 0.12f * (hitU * hitU + hitV * hitV))

            return Vec(hit.tint.x * falloff + hit.roomHere, hit.tint.y * falloff + hit.roomHere, hit.tint.z * falloff + hit.roomHere)
        }

        val g = roomLow + (roomHigh - roomLow) * smoothstep(-0.35f, 0.85f, d.y)

        return Vec(g, g, g)
    }

    /**
     * The whole cube on the CPU: per level, six faces of RGB floats, row 0 first. Slow at the shipped size, so
     * it is for a fallback cube a fraction of the size and for the tests.
     */
    fun bake(size: Int, levels: Int): List<List<FloatArray>> = (0 until levels).map { level ->
        val side = maxOf(size shr level, 1)
        val roughness = level.toFloat() / maxOf(levels - 1, 1)
        val samples = if (level == 0) emptyList() else samples(roughness)

        (0 until FACES).map { face ->
            FloatArray(side * side * 3).also { out ->
                for (j in 0 until side) {
                    for (i in 0 until side) {
                        val texel = if (level == 0) sharp(face, i, j, side) else prefiltered(face, i, j, side, samples)
                        val at = (j * side + i) * 3

                        out[at] = texel.x
                        out[at + 1] = texel.y
                        out[at + 2] = texel.z
                    }
                }
            }
        }
    }

    private fun sharp(face: Int, i: Int, j: Int, side: Int): Vec {
        var sum = Vec(0f, 0f, 0f)

        for (sy in 0..1) {
            for (sx in 0..1) {
                val s = 2f * (i + (sx + 0.5f) / 2f) / side - 1f
                val t = 2f * (j + (sy + 0.5f) / 2f) / side - 1f

                sum += radiance(faceDirection(face, s, t).normalized())
            }
        }

        return sum * 0.25f
    }

    private fun prefiltered(face: Int, i: Int, j: Int, side: Int, samples: List<Vec>): Vec {
        val s = 2f * (i + 0.5f) / side - 1f
        val t = 2f * (j + 0.5f) / side - 1f
        val n = faceDirection(face, s, t).normalized()
        val up = if (abs(n.z) < 0.999f) Vec(0f, 0f, 1f) else Vec(1f, 0f, 0f)
        val tangent = (up cross n).normalized()
        val bitangent = n cross tangent
        var sum = Vec(0f, 0f, 0f)
        var weight = 0f

        for (h in samples) {
            val half = tangent * h.x + bitangent * h.y + n * h.z
            val light = half * (2f * h.z) - n
            val nl = light dot n

            if (nl <= 0f) continue

            sum += radiance(light) * nl
            weight += nl
        }

        return sum * (1f / maxOf(weight, 1e-6f))
    }

    companion object {
        /** The sharp level's face size and how many levels of blur follow it; together they set `envMaxLod`. */
        const val SIZE = 1024
        const val LEVELS = 9
        const val FACES = 6

        fun faceDirection(face: Int, s: Float, t: Float): Vec = when (face) {
            0 -> Vec(1f, -t, -s)
            1 -> Vec(-1f, -t, s)
            2 -> Vec(s, 1f, t)
            3 -> Vec(s, -1f, -t)
            4 -> Vec(s, -t, 1f)
            else -> Vec(-s, -t, -1f)
        }

        /** How many GGX samples a level of perceptual roughness averages: few when nearly sharp, 1600 at the roughest. */
        fun sampleCount(roughness: Float): Int = (64 + 1536 * roughness * roughness).roundToInt()

        /** The GGX half vectors for a level, in tangent space, from the Hammersley set. */
        fun samples(roughness: Float): List<Vec> {
            val count = sampleCount(roughness)
            val alpha = roughness * roughness

            return List(count) { index ->
                val turn = (index + 0.5f) / count
                val spread = radicalInverse(index)
                val phi = 2f * PI.toFloat() * turn
                val cosTheta = sqrt((1f - spread) / (1f + (alpha * alpha - 1f) * spread))
                val sinTheta = sqrt(1f - cosTheta * cosTheta)

                Vec(sinTheta * cos(phi), sinTheta * sin(phi), cosTheta)
            }
        }

        /** Van der Corput radical inverse in base two of the 32-bit index. */
        fun radicalInverse(index: Int): Float = (Integer.reverse(index).toLong() and 0xFFFF_FFFFL).toFloat() / 4_294_967_296f

        private fun smoothstep(e0: Float, e1: Float, x: Float): Float {
            val t = ((x - e0) / (e1 - e0)).coerceIn(0f, 1f)

            return t * t * (3f - 2f * t)
        }
    }
}
