package io.paritytech.polkadotapp.common.presentation.screens

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.fragment.app.Fragment
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.fragment.findNavController
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrandProvider
import io.paritytech.polkadotapp.common.presentation.tabbar.LocalTabBarOffset
import io.paritytech.polkadotapp.common.presentation.tabbar.LocalTabBarVisibility
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarOffsetHolder
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarVisibilityHolder
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import javax.inject.Inject

abstract class BaseComposeFragment<T : BaseViewModel> : Fragment() {
    protected abstract val viewModel: T

    // Injected into every @AndroidEntryPoint subclass; expose the tab-bar state to screens.
    @Inject
    lateinit var tabBarOffsetHolder: TabBarOffsetHolder

    @Inject
    lateinit var tabBarVisibilityHolder: TabBarVisibilityHolder

    @Inject
    lateinit var appNotifier: AppNotifier

    @Inject
    lateinit var paymentAssetBrandProvider: PaymentAssetBrandProvider

    @Composable
    protected abstract fun Screen()

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View? = ComposeView(requireContext()).apply {
        setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)

        setContent {
            val paymentAssetBrand by paymentAssetBrandProvider.brand.collectAsStateWithLifecycle()

            CompositionLocalProvider(
                LocalTabBarOffset provides tabBarOffsetHolder.offset,
                LocalTabBarVisibility provides tabBarVisibilityHolder,
                LocalPaymentAssetBrand provides paymentAssetBrand,
            ) {
                PolkadotTheme {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .semantics { testTagsAsResourceId = true }
                    ) {
                        ObserveViewModelEvents(viewModel, appNotifier)

                        Screen()
                    }
                }
            }
        }
    }

    /**
     * Jetpack navigation specific observeResult functionality.
     */
    protected fun <T : Any> observeResult(
        key: String,
        lifecycleOwner: LifecycleOwner = viewLifecycleOwner,
        onResult: (T) -> Unit,
    ) {
        val navController = findNavController()
        navController.currentBackStackEntry?.savedStateHandle?.getLiveData<T>(key)?.observe(lifecycleOwner) { result ->
            onResult(result)
            navController.currentBackStackEntry?.savedStateHandle?.remove<T>(key)
        }
    }
}
