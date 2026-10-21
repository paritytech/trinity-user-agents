package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext

/** True when the system setting "Remove animations" is on. */
@Composable
internal fun rememberReducedMotion(): Boolean {
    val contentResolver = LocalContext.current.contentResolver

    return remember(contentResolver) {
        Settings.Global.getFloat(contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
}
