package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.compose.SimpleAssetListScreen
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleTokenListUiConfig
import javax.inject.Inject

abstract class SimpleAssetListFragment<VM : SimpleAssetListViewModel> : BaseComposeFragment<VM>() {
    @Inject
    lateinit var knownTokenFormatter: KnownTokenFormatter

    @Composable
    abstract fun config(): SimpleTokenListUiConfig

    @Composable
    override fun Screen() = CompositionLocalProvider(
        LocalKnownTokenFormatter provides knownTokenFormatter
    ) {
        SimpleAssetListScreen(viewModel, config())
    }
}
