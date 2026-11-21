package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier

/** Draws the material layers of a `Materials` node over its content, or the content alone. */
fun interface JsMaterialsDrawer {
    /** Draws [content], the children of the node, with the layers of the node numbered [index] in its tree. */
    @Composable
    fun Draw(modifier: Modifier, index: Int, content: @Composable () -> Unit)
}

/** The drawer of the surface the tree is drawn on. Without one, every `Materials` node draws its children plain. */
val LocalJsMaterialsDrawer = staticCompositionLocalOf<JsMaterialsDrawer?> { null }
