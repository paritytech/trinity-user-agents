package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel

/**
 * Example values the style and feel tests check against: a type by value, a 13-stop band of light, and a feel with a
 * press, a damped spring home, an approach and a pop table.
 */
internal object StyleFixtures {
    val font = JsModifier.Font(size = 18f, lineHeight = 22f, weight = 400, rgba = 0x585A62FFL, letterSpacing = 0f)

    val shimmer = JsModifier.Shimmer(
        rgba = 0xFFFFFFFFL,
        stops = listOf(0f, 0.011f, 0.054f, 0.181f, 0.44f, 0.766f, 0.97f, 0.766f, 0.44f, 0.181f, 0.054f, 0.011f, 0f),
        band = 186f,
        lead = 70f,
        speed = 212f,
        restMs = 1000,
    )

    val feel = JsPanFeel(
        pressScale = 1.059f,
        pressMs = 133,
        unpressMs = 150,
        springOmega = 18f,
        springDamping = 0.8f,
        approachMs = 117,
        approachReference = 42f,
        pop = listOf(
            1.059f, 1.132f, 1.192f, 1.225f, 1.235f, 1.237f, 1.237f, 1.237f, 1.232f, 1.226f, 1.219f, 1.198f, 1.149f, 1.099f,
            1.050f, 1.023f, 1.005f, 0.991f, 0.981f, 0.977f, 0.976f, 0.977f, 0.983f, 0.988f, 0.990f, 0.994f, 0.998f, 1.0f,
        ),
        popOffset = 3.33f,
    )
}
