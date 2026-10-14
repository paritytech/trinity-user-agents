package io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime

sealed interface RuntimeState {
    data object NotInitialized : RuntimeState
    data object Ready : RuntimeState
    data class Error(val cause: String) : RuntimeState
}
