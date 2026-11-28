package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.ProductId
import kotlin.time.Instant

// [key] tells apart two visits to the same screen.
data class FundingScreenEntry(val screen: FundingScreen, val key: Int)

interface FundingSheet {
    val intentId: FundingIntentId

    val entry: StateFlow<FundingEntry?>

    val flow: StateFlow<FundingFlowState?>

    val brands: StateFlow<Map<ProductId, FundingProviderBrand>>

    val now: StateFlow<Instant>

    fun update(transform: (FundingPicks) -> FundingPicks)

    fun requote()

    fun push(screen: FundingScreen)

    // A screen pushed over it since stays.
    fun pop(screen: FundingScreen)

    // A session that never started is dropped; one in flight runs on.
    fun close()

    fun message(@StringRes text: Int)
}

internal val WhileOnScreen = SharingStarted.WhileSubscribed(stopTimeoutMillis = 5_000)
