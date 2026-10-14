package io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun EmptyState() {
    Box(modifier = Modifier.fillMaxSize()) {
        NovaText(
            modifier = Modifier.align(Alignment.Center),
            text = stringResource(RCommon.string.products_empty_state_message)
        )
    }
}
