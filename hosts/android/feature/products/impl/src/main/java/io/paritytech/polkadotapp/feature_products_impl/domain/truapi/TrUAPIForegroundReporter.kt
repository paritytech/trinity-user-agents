package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import timber.log.Timber

/**
 * Tells the core if the app is in the foreground (VERBOSITY-WASM-PRODUCT A2). While it is, the core holds the worker
 * of each product that declares a foreground worker. The core starts in the background, so the first state goes to
 * the core when the runtime is built, and each change after it. [onForeground] runs at each return to the foreground.
 */
internal suspend fun Flow<AppLifecycleState>.reportForeground(
    setAppForeground: (Boolean) -> Unit,
    onForeground: () -> Unit,
) {
    map { it == AppLifecycleState.FOREGROUND }
        .distinctUntilChanged()
        .collect { foreground ->
            runCatching { setAppForeground(foreground) }
                .onSuccess { Timber.d("TrUAPI the app is in the %s", if (foreground) "foreground" else "background") }
                .onFailure { Timber.w(it, "TrUAPI the app state did not reach the core") }
            if (foreground) onForeground()
        }
}
