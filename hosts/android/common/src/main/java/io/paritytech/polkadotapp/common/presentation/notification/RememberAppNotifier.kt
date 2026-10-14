package io.paritytech.polkadotapp.common.presentation.notification

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import dagger.hilt.android.EntryPointAccessors
import io.paritytech.polkadotapp.common.di.modules.AppNotifierEntryPoint

@Composable
fun rememberAppNotifier(): AppNotifier {
    val context = LocalContext.current

    return remember(context) {
        EntryPointAccessors.fromApplication(
            context.applicationContext,
            AppNotifierEntryPoint::class.java
        ).appNotifier()
    }
}
