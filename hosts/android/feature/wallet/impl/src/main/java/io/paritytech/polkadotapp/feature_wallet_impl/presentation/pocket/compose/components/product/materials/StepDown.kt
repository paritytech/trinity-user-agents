package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

internal class StepDown {
    private var plain = false
    private var calmSince: Duration? = null

    fun update(now: Duration, strained: Boolean): Boolean {
        if (strained) {
            plain = true
            calmSince = null
        } else if (plain) {
            val since = calmSince ?: now.also { calmSince = it }
            if (now - since >= CALM_PERIOD) {
                plain = false
                calmSince = null
            }
        }
        return plain
    }

    private companion object {
        val CALM_PERIOD = 30.seconds
    }
}
