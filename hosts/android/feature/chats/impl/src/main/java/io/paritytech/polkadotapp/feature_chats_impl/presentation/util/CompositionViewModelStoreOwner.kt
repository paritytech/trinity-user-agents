package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.lifecycle.HasDefaultViewModelProviderFactory
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.viewmodel.CreationExtras
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner

// ViewModels built through the host's (Hilt) factory but cleared when the calling composable leaves the composition,
// for UI that comes and goes inside a long-lived host such as an Activity-level overlay.
@Composable
internal fun rememberCompositionViewModelStoreOwner(): ViewModelStoreOwner {
    val host = checkNotNull(LocalViewModelStoreOwner.current) { "No ViewModelStoreOwner was provided" }
    val owner = remember(host) { CompositionViewModelStoreOwner(host) }

    DisposableEffect(owner) {
        onDispose { owner.viewModelStore.clear() }
    }

    return owner
}

private class CompositionViewModelStoreOwner(
    private val host: ViewModelStoreOwner,
) : ViewModelStoreOwner, HasDefaultViewModelProviderFactory {
    override val viewModelStore = ViewModelStore()

    override val defaultViewModelProviderFactory: ViewModelProvider.Factory
        get() = (host as HasDefaultViewModelProviderFactory).defaultViewModelProviderFactory

    override val defaultViewModelCreationExtras: CreationExtras
        get() = (host as HasDefaultViewModelProviderFactory).defaultViewModelCreationExtras
}
