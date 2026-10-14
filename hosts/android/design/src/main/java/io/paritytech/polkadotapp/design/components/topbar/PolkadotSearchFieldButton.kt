package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotSearchFieldButton(
    modifier: Modifier = Modifier,
    placeholder: String?,
    onClick: () -> Unit,
) {
    PolkadotSearchField(
        modifier = modifier,
        value = "",
        onValueChange = {},
        onClear = {},
        enabled = false,
        placeholder = placeholder,
        onClick = onClick,
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun PolkadotSearchFieldButtonPreview() {
    PolkadotTheme {
        PolkadotSearchFieldButton(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.medium),
            placeholder = "Search",
            onClick = {},
        )
    }
}
