package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.VectorConverter
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imeAnimationTarget
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first

suspend fun liftDockWhileKeyboardLeaves(
    cardOpen: Flow<Boolean>,
    keyboardUp: Flow<Boolean>,
    onLift: (Boolean) -> Unit,
) {
    cardOpen.drop(1).collect { open ->
        if (!open && keyboardUp.first()) {
            onLift(true)
            keyboardUp.first { !it }
            onLift(false)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun rememberDockletKeyboardHeight(): () -> Dp {
    val density = LocalDensity.current
    val ime = WindowInsets.ime
    val imeTarget = WindowInsets.imeAnimationTarget
    val settling = remember { Animatable(0.dp, Dp.VectorConverter) }
    LaunchedEffect(imeTarget, density) {
        snapshotFlow { with(density) { imeTarget.getBottom(density).toDp() } }
            .collectLatest { target -> settling.animateTo(target, DockletMotion.stepChangeSpec()) }
    }
    val height = remember(ime, density) {
        derivedStateOf { max(settling.value, with(density) { ime.getBottom(density).toDp() }) }
    }
    return remember(height) { { height.value } }
}
