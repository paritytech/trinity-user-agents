package io.paritytech.polkadotapp.app.root.navigation.products

import androidx.navigation.NavController
import androidx.navigation.NavDestination
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import org.junit.Test

class ProductsNavigatorTest {
    private val navController: NavController = mockk()
    private val navigationHolder: NavigationHolder = mockk {
        every { navController } returns this@ProductsNavigatorTest.navController
    }

    private val navigator = ProductsNavigator(navigationHolder, mockk())

    @Test
    fun `a second leave once the browser is gone pops nothing`() {
        // Given
        withCurrentDestinations(R.id.spaBrowserFragment, R.id.mainFragment)

        // When
        navigator.leaveBrowser()
        navigator.leaveBrowser()

        // Then
        verify(exactly = 1) { navController.popBackStack() }
    }

    private fun withCurrentDestinations(vararg ids: Int) {
        every { navController.currentDestination } returnsMany ids.map { id -> mockk<NavDestination> { every { this@mockk.id } returns id } }
        every { navController.popBackStack() } returns true
    }
}
