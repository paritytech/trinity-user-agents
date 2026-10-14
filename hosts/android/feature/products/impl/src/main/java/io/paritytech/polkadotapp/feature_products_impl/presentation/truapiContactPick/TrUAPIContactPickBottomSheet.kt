package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.compose.TrUAPIContactPickScreen

/**
 * Stays dismissable, unlike the confirmation sheet: walking away from a picker
 * is an ordinary answer, and the ViewModel resolves it as naming nobody.
 */
@AndroidEntryPoint
class TrUAPIContactPickBottomSheet : BaseComposeBottomSheet<TrUAPIContactPickViewModel>() {
    override val viewModel: TrUAPIContactPickViewModel by viewModels()

    @Composable
    override fun Screen() {
        TrUAPIContactPickScreen(viewModel)
    }
}
