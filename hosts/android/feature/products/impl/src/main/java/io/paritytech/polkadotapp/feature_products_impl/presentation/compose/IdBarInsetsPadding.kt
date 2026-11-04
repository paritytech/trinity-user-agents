package io.paritytech.polkadotapp.feature_products_impl.presentation.compose

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.exclude
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.statusBar.LocalStatusBarScroll
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthBarDefaults

@Composable
fun Modifier.idBarInsetsPadding(insets: WindowInsets): Modifier {
    val pageUnderIdBar = LocalStatusBarScroll.current?.state?.collectAsStateWithLifecycle()?.value?.contentUnderBar == true
    val idBarOverlap = if (pageUnderIdBar) ChainHealthBarDefaults.ContentHeight else 0.dp

    return windowInsetsPadding(insets.exclude(WindowInsets(top = idBarOverlap)))
}
