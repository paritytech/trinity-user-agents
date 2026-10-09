package io.paritytech.polkadotapp.app.root.navigation.products

import androidx.core.os.bundleOf
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductSettingsPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.withContext
import javax.inject.Inject

class ProductsNavigator @Inject constructor(
    private val navigationHolder: NavigationHolder,
    private val dispatchers: CoroutineDispatchers,
) : BaseNavigator(navigationHolder), ProductsRouter {
    override suspend fun openSignTransaction() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_transactionSignBottomSheet)
    }

    override fun openSpaBrowser(payload: SpaBrowserPayload) = performNavigation(
        actionId = R.id.action_global_to_spaBrowserFragment,
        args = payload.toPayloadBundle(SpaBrowserPayload::class.java.name),
    )

    override fun leaveBrowser() {
        back()
    }

    override fun openProductChat(productId: ProductId) {
        performNavigation(
            actionId = R.id.action_global_to_chatFeedFragment,
            args = ChatFeedPayload.botChat(productId.toChatExtensionId()).toPayloadBundle(),
        )
    }

    override fun openChat(chatId: ChatId) {
        performNavigation(
            actionId = R.id.action_global_to_chatFeedFragment,
            args = ChatFeedPayload.existingChat(chatId).toPayloadBundle(),
        )
    }

    override fun openProductSettings(productId: ProductId) {
        performNavigation(
            actionId = R.id.action_productList_to_productSettings,
            args = ProductSettingsPayload(productId = productId.value).toPayloadBundle()
        )
    }

    override fun openProductPermissions(productId: ProductId) {
        performNavigation(
            actionId = R.id.action_productSettings_to_permissionSettings,
            args = ProductSettingsPayload(productId = productId.value).toPayloadBundle()
        )
    }

    override suspend fun openPermissionPrompt(requestId: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_permissionPromptBottomSheet,
            bundleOf(PermissionPromptBottomSheet.REQUEST_ID to requestId),
        )
    }

    override suspend fun closePermissionPrompt(requestId: String?) = withContext(dispatchers.main) {
        val controller = navigationHolder.navController
        val entry = controller?.currentBackStackEntry
        if (entry?.destination?.id == R.id.permissionPromptBottomSheet &&
            entry.arguments?.getString(PermissionPromptBottomSheet.REQUEST_ID) == requestId
        ) {
            controller.popBackStack()
        }
    }

    override suspend fun openPaymentRequestPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_paymentRequestBottomSheet)
    }

    override suspend fun openTopUpRequestPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_topUpRequestBottomSheet)
    }

    override suspend fun openResourceAllocationRequestPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_resourceAllocationRequestBottomSheet)
    }

    override suspend fun openCrossProductProofPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_crossProductProofBottomSheet)
    }

    override suspend fun openTrUAPIConfirmation() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_truapiConfirmationBottomSheet)
    }

    override suspend fun openTrUAPIContactPick() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_truapiContactPickBottomSheet)
    }

    override fun openPocketAddCard(payload: PocketAddCardPayload) = performNavigation(
        actionId = R.id.action_global_to_pocketAddCardBottomSheet,
        args = payload.toPayloadBundle(),
    )
}
