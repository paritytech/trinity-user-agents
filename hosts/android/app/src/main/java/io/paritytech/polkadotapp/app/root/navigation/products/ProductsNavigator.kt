package io.paritytech.polkadotapp.app.root.navigation.products

import androidx.core.os.bundleOf
import androidx.navigation.NavGraph
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
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended.FundingEndedBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingProviderFrameBottomSheet
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

    override suspend fun openFundingOverlay(intent: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_fundingBottomSheet,
            bundleOf(FundingBottomSheet.INTENT to intent),
        )
    }

    override suspend fun closeFundingOverlay(intent: String) = withContext(dispatchers.main) {
        popSession(R.id.fundingBottomSheet, FundingBottomSheet.INTENT, intent)
    }

    override suspend fun openFundingEnded(intent: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_fundingEndedBottomSheet,
            bundleOf(FundingEndedBottomSheet.INTENT to intent),
        )
    }

    override suspend fun closeFundingEnded(intent: String) = withContext(dispatchers.main) {
        popIfOnTop(R.id.fundingEndedBottomSheet, FundingEndedBottomSheet.INTENT, intent)
    }

    override suspend fun openFundingProviderFrame(intent: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_fundingProviderFrameBottomSheet,
            bundleOf(FundingProviderFrameBottomSheet.INTENT to intent),
        )
    }

    override suspend fun closeFundingProviderFrame(intent: String) = withContext(dispatchers.main) {
        popIfOnTop(R.id.fundingProviderFrameBottomSheet, FundingProviderFrameBottomSheet.INTENT, intent)
    }

    /**
     * Closes the session's sheet even when its provider frame sits on top of it: the frame belongs to the same
     * session, so it goes too. Anything else on top is left alone, and so is the sheet under it.
     */
    private fun popSession(destinationId: Int, argument: String, intent: String) {
        val controller = navigationHolder.navController ?: return
        val stack = controller.currentBackStack.value
        val index = stack.indexOfLast { it.destination.id == destinationId && it.arguments?.getString(argument) == intent }
        if (index < 0) return

        val above = stack.drop(index + 1).filter { it.destination !is NavGraph }
        val sameSession = above.all { it.arguments?.getString(FundingProviderFrameBottomSheet.INTENT) == intent }
        if (sameSession) controller.popBackStack(destinationId, inclusive = true)
    }

    private fun popIfOnTop(destinationId: Int, argument: String, value: String) {
        val controller = navigationHolder.navController
        val entry = controller?.currentBackStackEntry
        if (entry?.destination?.id == destinationId && entry.arguments?.getString(argument) == value) {
            controller.popBackStack()
        }
    }

    override suspend fun closeTrUAPIContactPick() = closeIfShowing(R.id.truapiContactPickBottomSheet)

    override suspend fun openTrUAPIProductScan() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_truapiProductScanBottomSheet)
    }

    override suspend fun closeTrUAPIProductScan() = closeIfShowing(R.id.truapiProductScanBottomSheet)

    /**
     * Closes the prompt only when it is on top, so a sheet over it stays. A prompt under another
     * sheet closes itself when it is shown again, because its answer is already given.
     */
    private suspend fun closeIfShowing(destinationId: Int) = withContext(dispatchers.main) {
        if (isCurrentDestination(destinationId)) back()
    }

    override fun openPocketAddCard(payload: PocketAddCardPayload) = performNavigation(
        actionId = R.id.action_global_to_pocketAddCardBottomSheet,
        args = payload.toPayloadBundle(),
    )
}
