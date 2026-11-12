package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.abs
import kotlin.math.ln
import kotlin.math.pow

/**
 * What a denomination looks like as a coin: its metal, its outline and its size.
 *
 * Banded the way a circulating coinage is. Four consecutive denominations share a metal and run through the
 * same four outlines, so the metal tells you roughly what a coin is worth and the outline tells you which of
 * the four it is. Size climbs gently across the whole range, enough to separate neighbours without turning
 * the field into a bar chart.
 */
object CoinageCoinDesign {
    /** The four metals, lowest band first. The top band is bimetallic, where a real coinage puts its largest. */
    enum class Band { BRONZE, SILVER, GOLD, TWIN }

    /** Outline family. Every one of these exists on a real coin somewhere. */
    sealed interface Shape {
        data object Round : Shape

        /** Equilateral curved polygon of constant width, as on the UK 20p and 50p. */
        data class CurvedPolygon(val sides: Int, val rounding: Float) : Shape

        /** Spain's 20 centimos: a round edge with smooth indents. */
        data class Flower(val notches: Int, val depth: Float, val width: Float, val rounding: Float) : Shape

        /** Convex lobes meeting in rounded cusps. */
        data class Scalloped(val lobes: Int, val lobe: Float, val rounding: Float) : Shape
    }

    data class Design(
        val shape: Shape,
        /** Diameter as a fraction of the largest denomination's. */
        val size: Float,
        /** Whether the edge is milled. */
        val isReeded: Boolean
    )

    /** Exponents `0..14`, four to a band. The top band is one short, which is the chain's doing. */
    const val DENOMINATIONS_PER_BAND = 4
    const val HIGHEST_EXPONENT = 14

    fun band(exponent: Int): Band {
        val clamped = exponent.coerceIn(0, HIGHEST_EXPONENT)

        return Band.entries[(clamped / DENOMINATIONS_PER_BAND).coerceAtMost(Band.entries.lastIndex)]
    }

    fun design(exponent: Int): Design {
        val clamped = exponent.coerceIn(0, HIGHEST_EXPONENT)
        val shape = SHAPES[clamped % DENOMINATIONS_PER_BAND]

        return Design(
            shape = shape,
            size = size(clamped),
            // Milling is what a mint adds to coins worth protecting, so the coppers go without.
            isReeded = band(exponent) != Band.BRONZE && shape == Shape.Round
        )
    }

    /** The same four outlines in every band, so a coin's shape names its place within its metal. */
    private val SHAPES = listOf(
        Shape.Round,
        Shape.CurvedPolygon(sides = 7, rounding = 0.035f),
        Shape.Flower(notches = 7, depth = 0.04f, width = 0.15f, rounding = 0.055f),
        Shape.Scalloped(lobes = 10, lobe = 0.15f, rounding = 0.012f)
    )

    private fun size(exponent: Int): Float = 0.82f + 0.18f * (exponent.toFloat() / HIGHEST_EXPONENT)
}

/**
 * How traceable a coin still is, as the reference renderer measures it.
 *
 * A level counts doublings of the crowd a coin hides in: level 0 is alone, level `n` means fewer than `2^n`
 * others. The difference between 510 others and 511 is nothing; between one and four it is most of the
 * story, so the early steps move fastest.
 */
object CoinageWear {
    /** A recycler ring holds 767 keys, so a coin can be fungible with at most 766 others. */
    const val RING_CAPACITY = 767

    val maximumLevel = level(hiddenAmong = RING_CAPACITY - 1)

    fun level(hiddenAmong: Int): Int = if (hiddenAmong <= 0) 0 else (ln(hiddenAmong.toDouble()) / LN_2).toInt() + 1

    fun amount(level: Int): Float {
        val progress = (level.toFloat() / maximumLevel).coerceIn(0f, 1f)

        return (1f - progress).toDouble().pow(1.15).toFloat()
    }

    /**
     * The level a wear amount came from, for grouping. [amount] is monotone in level, so this is the nearest
     * level rather than an inverse that could miss by rounding.
     */
    fun level(amount: Float): Int = (0..maximumLevel).minByOrNull { abs(this.amount(it) - amount) } ?: 0

    /**
     * Wear for a holding whose recycler we have no record of, which is every coin received from a peer.
     * Nothing can be credited, so it wears as though it hides among nobody.
     */
    val unknown = amount(0)
}

private val LN_2 = ln(2.0)
