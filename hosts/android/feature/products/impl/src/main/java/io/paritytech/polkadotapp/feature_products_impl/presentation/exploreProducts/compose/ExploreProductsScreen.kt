package io.paritytech.polkadotapp.feature_products_impl.presentation.exploreProducts.compose

import android.webkit.WebView
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.viewinterop.AndroidView
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.exploreProducts.ExploreProductsViewModel

@Composable
fun ExploreProductsScreen() {
    val viewModel = hiltViewModel<ExploreProductsViewModel>()

    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { viewModel.resumeConnections() }
    LifecycleEventEffect(Lifecycle.Event.ON_PAUSE) { viewModel.pauseConnections() }

    val webView by viewModel.webViewFlow.collectAsStateWithLifecycle()

    ExploreProductsScreenInternal(
        webView = webView
    )
}

@Composable
private fun ExploreProductsScreenInternal(
    webView: WebView?
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(
            modifier = Modifier
                .statusBarsPadding()
                .windowInsetsPadding(LocalAppNavigationBarInsets.current)
        ) {
            if (webView != null) {
                AndroidView(
                    modifier = Modifier.fillMaxSize(),
                    factory = { webView },
                )
            }
        }
    }
}

@Preview
@Composable
private fun ExploreProductsScreenPreview() {
    PolkadotTheme {
        ExploreProductsScreenInternal(
            webView = null
        )
    }
}
