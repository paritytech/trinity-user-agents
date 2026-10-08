package io.paritytech.polkadotapp.app.root.navigation.products

import androidx.annotation.IdRes
import androidx.core.os.bundleOf
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductSettingsPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaSheetPayload
import io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof.CrossProductProofBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest.ResourceAllocationRequestBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.TransactionSignBottomSheet
import kotlinx.coroutines.withContext
import javax.inject.Inject

class ProductsNavigator @Inject constructor(
    private val navigationHolder: NavigationHolder,
    private val dispatchers: CoroutineDispatchers,
) : BaseNavigator(navigationHolder), ProductsRouter {
    override suspend fun openSignTransaction(requestId: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_transactionSignBottomSheet,
            bundleOf(TransactionSignBottomSheet.REQUEST_ID to requestId),
        )
    }

    override suspend fun closeSignTransaction(requestId: String) =
        closeIfOnTop(R.id.transactionSignBottomSheet, TransactionSignBottomSheet.REQUEST_ID, requestId)

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

    override suspend fun closePermissionPrompt(requestId: String?) =
        closeIfOnTop(R.id.permissionPromptBottomSheet, PermissionPromptBottomSheet.REQUEST_ID, requestId)

    override suspend fun openPaymentRequestPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_paymentRequestBottomSheet)
    }

    override suspend fun openTopUpRequestPrompt() = withContext(dispatchers.main) {
        performNavigation(R.id.action_global_to_topUpRequestBottomSheet)
    }

    override suspend fun openResourceAllocationRequestPrompt(requestId: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_resourceAllocationRequestBottomSheet,
            bundleOf(ResourceAllocationRequestBottomSheet.REQUEST_ID to requestId),
        )
    }

    override suspend fun closeResourceAllocationRequestPrompt(requestId: String) =
        closeIfOnTop(R.id.resourceAllocationRequestBottomSheet, ResourceAllocationRequestBottomSheet.REQUEST_ID, requestId)

    override suspend fun openCrossProductProofPrompt(requestId: String) = withContext(dispatchers.main) {
        performNavigation(
            R.id.action_global_to_crossProductProofBottomSheet,
            bundleOf(CrossProductProofBottomSheet.REQUEST_ID to requestId),
        )
    }

    override suspend fun closeCrossProductProofPrompt(requestId: String) =
        closeIfOnTop(R.id.crossProductProofBottomSheet, CrossProductProofBottomSheet.REQUEST_ID, requestId)

    // Pops only the named request's own sheet: a sheet stacked above it, or one that already went, is left alone
    private suspend fun closeIfOnTop(@IdRes destination: Int, requestIdKey: String, requestId: String?) =
        withContext(dispatchers.main) {
            val controller = navigationHolder.navController
            val entry = controller?.currentBackStackEntry
            if (entry?.destination?.id == destination && entry.arguments?.getString(requestIdKey) == requestId) {
                controller.popBackStack()
            }
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

    override fun openPocketCard(key: PocketCardKey) = performNavigation(
        actionId = R.id.action_global_to_spaSheetBottomSheet,
        args = SpaSheetPayload(key.launchUrl()).toPayloadBundle(),
    )
}
