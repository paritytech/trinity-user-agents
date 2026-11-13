package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.ProductId

/** One screen on the funding sheet's stack; [key] tells apart two visits to the same screen. */
data class FundingScreenEntry(val screen: FundingScreen, val key: Int)

/**
 * The funding sheet one session is shown in, as its screens see it: the session as the core holds it, what the user
 * picked so far, and the sheet's stack. Each screen's view model reads only what its screen draws.
 */
interface FundingSheet {
    val intent: String

    /** The session as the funding service last reported it; `null` until it has. */
    val entry: StateFlow<FundingEntry?>

    /** What the user picked, with the core's options and quotes for it; `null` until the session first arrives. */
    val flow: StateFlow<FundingFlowState?>

    val brands: StateFlow<Map<ProductId, FundingProviderBrand>>

    fun update(transform: (FundingFlowState) -> FundingFlowState)

    /** Reports the selection on screen, then asks quotes of its amount. */
    fun requote()

    fun push(screen: FundingScreen)

    /** Takes [screen] off the stack while it is on top; a screen pushed over it since stays. */
    fun pop(screen: FundingScreen)

    /** Leaves the sheet. A session that never started is dropped; one in flight runs on. */
    fun close()

    fun message(@StringRes text: Int)
}

/** A screen's state is built only while its screen is on the sheet. */
internal val WhileOnScreen = SharingStarted.WhileSubscribed(stopTimeoutMillis = 5_000)
