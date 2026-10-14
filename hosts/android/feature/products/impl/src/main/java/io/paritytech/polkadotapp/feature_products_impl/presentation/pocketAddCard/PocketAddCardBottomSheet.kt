package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard.compose.PocketAddCardScreen

@AndroidEntryPoint
class PocketAddCardBottomSheet : BaseComposeBottomSheet<PocketAddCardViewModel>() {
    override val viewModel: PocketAddCardViewModel by viewModels()

    @Composable
    override fun Screen() = PocketAddCardScreen(viewModel)
}
