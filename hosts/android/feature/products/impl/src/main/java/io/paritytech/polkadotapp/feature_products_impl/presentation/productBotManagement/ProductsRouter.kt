package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.common.presentation.navigation.TabRouter
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRouter
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import uniffi.truapi.ProductId

interface ProductsRouter : ReturnableRouter, SigningRouter, TabRouter {
    fun openSpaBrowser(payload: SpaBrowserPayload)

    /** Leave the browser for the main screen, which resumes on its last selected bottom tab. */
    fun leaveBrowser()
    fun openProductChat(productId: ProductId)
    fun openChat(chatId: ChatId)
    suspend fun openPermissionPrompt(requestId: String)
    suspend fun closePermissionPrompt(requestId: String?)
    suspend fun openPaymentRequestPrompt()
    suspend fun openTopUpRequestPrompt()
    suspend fun openResourceAllocationRequestPrompt()
    suspend fun openCrossProductProofPrompt()

    /** Confirmation prompt for an action the TrUAPI Rust core is about to take. */
    suspend fun openTrUAPIConfirmation()

    /** Picker for the one contact a product asked the user to name. */
    suspend fun openTrUAPIContactPick()
    suspend fun closeTrUAPIContactPick()

    /** Viewfinder for the one code a product asked the user to scan. */
    suspend fun openTrUAPIProductScan()
    suspend fun closeTrUAPIProductScan()

    /** The funding sheet for the session the core is waiting on. */
    suspend fun openFundingOverlay(intentId: FundingIntentId)

    /**
     * Takes down the funding sheet for [intentId], with its provider screen when that sits on top of it; anything else on
     * top is left alone, and so is the sheet under it.
     */
    suspend fun closeFundingOverlay(intentId: FundingIntentId)

    suspend fun openFundingEnded(intentId: FundingIntentId)

    suspend fun closeFundingEnded(intentId: FundingIntentId)

    /** Every funding session, in flight and ended, over whatever is on screen. */
    suspend fun openFundingHistory()

    /** Whether the funding sheet for [intentId] is on top, alone or under its own provider screen. */
    suspend fun isFundingOverlayShown(intentId: FundingIntentId): Boolean

    /** A funding provider's own screen, full height, showing [request] over its session. */
    suspend fun openFundingProviderFrame(request: FundingFrameRequest)

    /** Takes down the provider screen for [intentId], when it is the screen on top. */
    suspend fun closeFundingProviderFrame(intentId: FundingIntentId)

    fun openProductSettings(productId: ProductId)
    fun openProductPermissions(productId: ProductId)

    /** Approval sheet for a card the user was offered through a Pocket deeplink. */
    fun openPocketAddCard(payload: PocketAddCardPayload)
}
