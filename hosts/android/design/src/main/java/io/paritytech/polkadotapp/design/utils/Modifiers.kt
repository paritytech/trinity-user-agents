package io.paritytech.polkadotapp.design.utils

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * Conditionally applies a modifier based on a nullable value.
 * If the value is not null, applies the onNotNull modifier with the value.
 */
@Composable
fun <T : Any> Modifier.conditionalNotNull(
    value: T?,
    onNotNull: @Composable Modifier.(T) -> Modifier,
): Modifier = value?.let { onNotNull(it) } ?: this

inline fun Modifier.modifyIf(
    condition: Boolean,
    modification: Modifier.() -> Modifier,
): Modifier =
    if (condition) {
        modification()
    } else {
        this
    }
