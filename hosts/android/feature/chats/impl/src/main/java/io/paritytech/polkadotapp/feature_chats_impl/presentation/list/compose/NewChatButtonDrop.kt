package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose

import kotlin.math.roundToInt

internal fun newChatButtonDrop(barInsetBottom: Int, navigationBarsBottom: Int, hiddenFraction: Float): Int =
    ((barInsetBottom - navigationBarsBottom).coerceAtLeast(0) * hiddenFraction).roundToInt()
