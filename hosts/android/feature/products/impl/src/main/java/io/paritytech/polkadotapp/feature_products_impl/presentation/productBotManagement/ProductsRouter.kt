package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.common.presentation.navigation.TabRouter
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRouter
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload

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
    fun openProductSettings(productId: ProductId)
    fun openProductPermissions(productId: ProductId)

    /** Approval sheet for a card the user was offered through a Pocket deeplink. */
    fun openPocketAddCard(payload: PocketAddCardPayload)
}
