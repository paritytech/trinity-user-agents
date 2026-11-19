package io.paritytech.polkadotapp.app.root.navigation

import android.os.Bundle
import androidx.annotation.IdRes
import androidx.navigation.NavController
import androidx.navigation.NavGraph
import androidx.navigation.NavOptions
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.common.presentation.navigation.TabRouter
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab

abstract class BaseNavigator(private val navigationHolder: NavigationHolder) : ReturnableRouter, TabRouter {
    override fun back() {
        navigationHolder.executeBack()
    }

    /**
     * Jetpack navigation specific setResult functionality
     */
    override fun <T : Any> backWithResult(key: String, result: T) {
        val navController = navigationHolder.navController ?: return
        val previousEntry = navController.previousBackStackEntry

        navController.popBackStack()

        previousEntry?.savedStateHandle[key] = result
    }

    /**
     * Performs conditional navigation based on current destination
     * @param cases - array of pairs (currentDestination, navigationAction)
     */
    fun performNavigation(
        cases: Array<Pair<Int, Int>>,
        args: Bundle? = null,
        navOptions: NavOptions? = null
    ) {
        val navController = navigationHolder.navController

        navController?.currentDestination?.let { currentDestination ->
            val (_, case) =
                cases.find { (startDestination, _) -> startDestination == currentDestination.id }
                    ?: throw IllegalArgumentException("Unknown case for ${currentDestination.label}")

            currentDestination.getAction(case)?.let {
                navController.navigate(case, args, navOptions)
            }
        }
    }

    override fun openChatsTab() = openTab(BottomTab.CHATS)

    override fun openWalletTab() = openTab(BottomTab.WALLET)

    override fun openExploreTab() = openTab(BottomTab.EXPLORE)

    override fun openSettingsTab() = openTab(BottomTab.SETTINGS)

    // The tabs live on the main screen; switched from a screen above it, the tab would change out of sight.
    private fun openTab(tab: BottomTab) {
        popBackstack(R.id.mainFragment)
        navigationHolder.requestTab(tab)
    }

    protected fun performNavigation(
        @IdRes actionId: Int,
        args: Bundle? = null,
        navOptions: NavOptions? = null
    ) {
        val navController = navigationHolder.navController

        navController?.performNavigation(actionId, args, navOptions)
    }

    protected fun performNavigationToGraph(
        @IdRes actionId: Int,
        @IdRes graphId: Int,
        @IdRes startDestinationId: Int,
        args: Bundle? = null,
        navOptions: NavOptions? = null
    ) {
        val navController = navigationHolder.navController ?: return
        val graph = navController.graph.findNode(graphId) as? NavGraph
            ?: error("Navigation graph $graphId was not found")

        graph.setStartDestination(startDestinationId)
        navController.performNavigation(actionId, args, navOptions)
    }

    /** Returns whether [destinationId] was on the back stack and everything above it got popped. */
    protected fun popBackstack(@IdRes destinationId: Int, inclusive: Boolean = false): Boolean {
        val navController = navigationHolder.navController
        return navController?.popBackStack(destinationId, inclusive) ?: false
    }

    protected fun isCurrentDestination(@IdRes destinationId: Int): Boolean =
        navigationHolder.navController?.currentDestination?.id == destinationId

    protected fun NavController.performNavigation(
        @IdRes actionId: Int,
        args: Bundle? = null,
        navOptions: NavOptions? = null
    ) {
        currentDestination?.getAction(actionId)?.let {
            navigate(actionId, args, navOptions)
        }
    }
}
