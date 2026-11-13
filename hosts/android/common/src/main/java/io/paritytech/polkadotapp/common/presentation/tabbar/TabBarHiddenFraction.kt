package io.paritytech.polkadotapp.common.presentation.tabbar

import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TabBarHiddenFractionHolder @Inject constructor() {
    val fraction: StateFlow<Float>
        field = MutableStateFlow(0f)

    fun setFraction(value: Float) {
        fraction.value = value
    }
}

val LocalTabBarHiddenFraction = staticCompositionLocalOf<StateFlow<Float>> { MutableStateFlow(0f) }
