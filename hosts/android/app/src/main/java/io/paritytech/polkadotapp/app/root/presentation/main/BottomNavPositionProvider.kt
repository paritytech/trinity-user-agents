package io.paritytech.polkadotapp.app.root.presentation.main

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class BottomNavPositionProvider @Inject constructor() {
    private val mutableHeightDp = MutableStateFlow(0.dp)
    val heightDp: StateFlow<Dp> = mutableHeightDp

    private val mutableHiddenFraction = MutableStateFlow(0f)
    val hiddenFraction: StateFlow<Float> = mutableHiddenFraction

    fun setHeight(heightDp: Dp) {
        mutableHeightDp.value = heightDp
    }

    fun setHiddenFraction(fraction: Float) {
        mutableHiddenFraction.value = fraction
    }

    fun clear() {
        mutableHeightDp.value = 0.dp
        mutableHiddenFraction.value = 0f
    }
}
