package io.paritytech.polkadotapp.app.root.presentation.main

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class BottomNavHeightProvider @Inject constructor() {
    private val mutableHeightDp = MutableStateFlow(0.dp)
    val heightDp: StateFlow<Dp> = mutableHeightDp

    fun set(heightDp: Dp) {
        mutableHeightDp.value = heightDp
    }

    fun clear() {
        mutableHeightDp.value = 0.dp
    }
}
