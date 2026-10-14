package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview.compose.PocketFacePreviewScreen

@AndroidEntryPoint
class PocketFacePreviewFragment : BaseComposeFragment<PocketFacePreviewViewModel>() {
    override val viewModel: PocketFacePreviewViewModel by viewModels()

    @Composable
    override fun Screen() {
        PocketFacePreviewScreen(viewModel)
    }
}
