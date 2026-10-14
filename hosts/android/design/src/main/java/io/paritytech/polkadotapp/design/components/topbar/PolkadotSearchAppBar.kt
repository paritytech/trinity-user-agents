package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Composable
fun PolkadotSearchAppBar(
    value: String,
    onValueChange: (String) -> Unit,
    onClear: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    actions: ImmutableList<TopBarAction> = persistentListOf(),
    focusRequester: FocusRequester? = null,
) {
    PolkadotTopBar(
        modifier = modifier,
        actions = actions,
    ) {
        PolkadotSearchField(
            modifier = Modifier.fillMaxWidth(),
            value = value,
            onValueChange = onValueChange,
            onClear = onClear,
            placeholder = placeholder,
            leadingIcon = NovaIcons.ArrowLeft,
            onLeadingClick = onBack,
            focusRequester = focusRequester,
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun PolkadotSearchAppBarPreview() {
    PolkadotTheme {
        PolkadotSearchAppBar(
            value = "Input text",
            onValueChange = {},
            onClear = {},
            onBack = {},
        )
    }
}
