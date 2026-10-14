package io.paritytech.polkadotapp.common.presentation.tabbar

import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

/** The bar's resting intrusion — the width of its always-visible nub. */
val TabBarBaseInset: Dp = 16.dp

/**
 * How far the global navigation bar currently intrudes from the right edge — its visible width in dp (the
 * [TabBarBaseInset] nub when collapsed, up to the full bar width when pulled out). The bar writes it; screens
 * read it (via [TabBarOffset]) to shift their own UI aside, e.g. slide a message input off-screen.
 */
@Singleton
class TabBarOffsetHolder @Inject constructor() {
    val offset: StateFlow<Dp>
        field = MutableStateFlow(TabBarBaseInset)

    fun setOffset(value: Dp) {
        offset.value = value
    }
}

/** Provided once around every screen's content (in the base compose fragment). */
val LocalTabBarOffset = staticCompositionLocalOf<StateFlow<Dp>> { MutableStateFlow(TabBarBaseInset) }
