package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRouter
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload

interface ProductsRouter : ReturnableRouter, SigningRouter {
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

    fun openProductSettings(productId: ProductId)
    fun openProductPermissions(productId: ProductId)

    /** Approval sheet for a card the user was offered through a Pocket deeplink. */
    fun openPocketAddCard(payload: PocketAddCardPayload)

    /**
     * Opens the product behind a card the user followed a link to, in a sheet, with the card named
     * in its launch URL. Tapping the same card on the Pocket tab expands it in place instead; a
     * deeplink has no card on screen to expand, and lands on the product's page directly.
     */
    fun openPocketCard(key: PocketCardKey)
}
