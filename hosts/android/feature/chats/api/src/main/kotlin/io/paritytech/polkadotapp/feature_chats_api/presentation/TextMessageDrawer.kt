package io.paritytech.polkadotapp.feature_chats_api.presentation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.common.domain.model.Timestamp

@Immutable
interface TextMessageDrawer {
    @Composable
    fun Draw(
        modifier: Modifier,
        text: String,
        isOutgoing: Boolean = true,
        timestamp: Timestamp = System.currentTimeMillis(),
        repliedTo: String? = null,
        repliedText: String? = null
    )
}
