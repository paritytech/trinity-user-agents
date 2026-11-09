package io.paritytech.polkadotapp.design.components.docklet

import kotlin.math.PI
import kotlin.math.exp
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

internal object DockletGlowProfile {
    private const val EndRadius = 22f
    private const val SharpWeight = 0.5f
    private const val SharpDecay = 4.2f
    private const val SoftWeight = 0.06f
    private const val SoftDecay = 14f
    private const val BlurSigma = 2.4f
    private const val IntegrationStep = 0.05f
    private const val LookupStep = 0.25f
    private const val MinWidthFraction = 0.5f

    private val blurredLookup = FloatArray((DockletDrawbridge.GlowWidth / LookupStep).roundToInt() + 1) { blurredAlpha(it * LookupStep) }

    fun alpha(inFromEdge: Float): Float {
        if (inFromEdge >= DockletDrawbridge.GlowWidth) return 0f
        return SharpWeight * exp(-inFromEdge / SharpDecay) + SoftWeight * exp(-inFromEdge / SoftDecay) * (1f - inFromEdge / DockletDrawbridge.GlowWidth)
    }

    private fun blurredAlpha(inFromEdge: Float): Float {
        val steps = (DockletDrawbridge.GlowWidth / IntegrationStep).roundToInt()
        var total = 0f
        for (step in 0..steps) {
            val offset = step * IntegrationStep
            total += alpha(offset) * gaussian(inFromEdge - offset) * IntegrationStep
        }
        return total
    }

    fun taperedDistance(inFromEdge: Float, along: Float, length: Float): Float {
        val fromNearerEnd = max(0f, min(along, length - along))
        val radius = min(EndRadius, length / 2f)
        if (radius <= 0f) return DockletDrawbridge.GlowWidth
        if (fromNearerEnd >= radius) return inFromEdge
        return hypot(inFromEdge, DockletDrawbridge.GlowWidth * (radius - fromNearerEnd) / radius)
    }

    fun glowAlpha(inFromEdge: Float, along: Float, length: Float): Float {
        val tapered = taperedDistance(inFromEdge, along, length)
        if (tapered >= DockletDrawbridge.GlowWidth) return 0f
        val index = tapered / LookupStep
        val lower = index.toInt()
        return blurredLookup[lower] + (blurredLookup[lower + 1] - blurredLookup[lower]) * (index - lower)
    }

    fun shimmerStrength(inFromEdge: Float, along: Float, length: Float): Float =
        max(0f, 1f - taperedDistance(inFromEdge, along, length) / DockletDrawbridge.GlowWidth)

    fun widthScale(progress: Float): Float = MinWidthFraction + (1f - MinWidthFraction) * progress.coerceIn(0f, 1f)

    private fun gaussian(offset: Float): Float =
        exp(-offset * offset / (2f * BlurSigma * BlurSigma)) / (BlurSigma * sqrt(2f * PI.toFloat()))
}
