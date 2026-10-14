package io.paritytech.polkadotapp.app.root.navigation.root

import androidx.core.os.bundleOf
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.ISSUE_SCREENSHOT_PATH
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import javax.inject.Inject

class RootNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), RootRouter {
    override fun openMain() {
        if (isCurrentDestination(R.id.mainFragment)) return
        performNavigation(R.id.action_global_to_main_graph)
    }

    override fun openActiveProduct() = performNavigation(R.id.action_global_to_spaBrowserFragment)

    override fun openClaimUsername() = performNavigationToGraph(
        actionId = R.id.action_global_to_claim_username_graph,
        graphId = R.id.claim_username_graph,
        startDestinationId = R.id.claimUsernameFragment
    )

    override fun openDebugMenu() = performNavigation(R.id.action_global_to_debug_menu)

    override fun openIssueReport(screenshotPath: String) = performNavigation(
        R.id.action_global_to_reportIssueBottomSheet,
        args = bundleOf(ISSUE_SCREENSHOT_PATH to screenshotPath),
    )

    override fun openProductBotsManagement() = performNavigation(R.id.action_global_to_product_bots_management)

    override fun openPocketFacePreview() = performNavigation(R.id.action_global_to_pocket_face_preview)

    override fun openSpaBrowser(payload: SpaBrowserPayload) = performNavigation(
        R.id.action_global_to_spaBrowserFragment,
        args = payload.toPayloadBundle(SpaBrowserPayload::class.java.name),
    )
}
