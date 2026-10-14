package io.paritytech.polkadotapp.feature_dotns_api.domain

sealed interface DotNsLoadProgress {
    data object Idle : DotNsLoadProgress

    data object Resolving : DotNsLoadProgress

    data class Downloading(val fraction: Float?) : DotNsLoadProgress

    data object Unpacking : DotNsLoadProgress

    data object Completed : DotNsLoadProgress

    data class Failed(val cause: Throwable) : DotNsLoadProgress
}
