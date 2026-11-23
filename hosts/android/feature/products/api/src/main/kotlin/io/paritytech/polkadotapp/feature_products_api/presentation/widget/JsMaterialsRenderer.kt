package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier

/** How a surface draws a `Materials` node: its shader layers over [Node]'s children, or the children alone. */
fun interface JsMaterialsRenderer {
    /** Draws node [node], the tree's [node]th `Materials` node depth first, over [children]. */
    @Composable
    fun Node(modifier: Modifier, node: Int, children: @Composable () -> Unit)
}

/** Draws every `Materials` node plain, as a box around its children, unless a surface provides its own. */
val LocalJsMaterialsRenderer = staticCompositionLocalOf {
    JsMaterialsRenderer { modifier, _, children ->
        Box(modifier = modifier, contentAlignment = Alignment.TopStart) { children() }
    }
}

/** Called when an image in the tree finishes loading, so a surface that pictures the tree takes a new picture. */
val LocalJsImageLoaded = staticCompositionLocalOf<() -> Unit> { {} }
