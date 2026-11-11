package io.paritytech.polkadotapp.design.utils

import android.animation.ValueAnimator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember

/** Whether the system plays animations: off when the user set the animator duration scale to zero. */
fun systemAnimationsEnabled(): Boolean = ValueAnimator.areAnimatorsEnabled()

/** Whether the user turned animations off, which leaves an animation at its end state. Read once per composition. */
@Composable
fun rememberReducedMotion(): Boolean = remember { !systemAnimationsEnabled() }
