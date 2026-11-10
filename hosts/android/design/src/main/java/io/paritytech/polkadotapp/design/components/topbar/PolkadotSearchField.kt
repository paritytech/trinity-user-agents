package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Search
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.PolkadotInputField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.conditionalNotNull

private val SearchSlotSize = 48.dp

@Composable
fun PolkadotSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    placeholder: String? = null,
    leadingIcon: ImageVector = NovaIcons.Search,
    onLeadingClick: (() -> Unit)? = null,
    imeAction: ImeAction = ImeAction.Search,
    focusRequester: FocusRequester? = null,
    onClick: (() -> Unit)? = null,
    showClear: Boolean = value.isNotEmpty(),
    contentPadding: PaddingValues = PaddingValues(PolkadotTheme.spacings.tiny),
    clearButtonSize: PolkadotIconButtonSize = PolkadotIconButtonSize.mediumIncreased(),
) {
    val placeholderContent: (@Composable () -> Unit)? = placeholder?.let { text ->
        {
            NovaText(
                text = text,
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary,
            )
        }
    }

    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.action.tertiary,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
        contentAlignment = Alignment.CenterStart,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(contentPadding),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
        ) {
            // Figma renders the leading slot as an icon button: clickable (e.g. a back arrow in the
            // app-bar search) when [onLeadingClick] is set, otherwise a decorative search glyph.
            if (onLeadingClick != null) {
                PolkadotIconButton(
                    icon = leadingIcon,
                    onClick = onLeadingClick,
                    style = PolkadotButtonStyle.ghost(),
                    size = PolkadotIconButtonSize.mediumIncreased(),
                    shape = PolkadotButtonShape.pill,
                )
            } else {
                Box(
                    modifier = Modifier.size(SearchSlotSize),
                    contentAlignment = Alignment.Center,
                ) {
                    NovaIcon(
                        imageVector = leadingIcon,
                        tint = PolkadotTheme.colors.fg.tertiary,
                    )
                }
            }

            PolkadotInputField(
                modifier = Modifier
                    .weight(1f)
                    .conditionalNotNull(focusRequester) { focusRequester(it) },
                value = value,
                onValueChange = onValueChange,
                enabled = enabled,
                placeholder = placeholderContent,
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    imeAction = imeAction,
                ),
            )

            if (showClear) {
                Box(
                    modifier = Modifier.size(SearchSlotSize),
                    contentAlignment = Alignment.Center,
                ) {
                    PolkadotIconButton(
                        icon = NovaIcons.Close,
                        onClick = onClear,
                        style = PolkadotButtonStyle.ghost(),
                        size = clearButtonSize,
                        shape = PolkadotButtonShape.pill,
                    )
                }
            }
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun PolkadotSearchFieldPreview() {
    PolkadotTheme {
        Column {
            Row(modifier = Modifier.padding(PolkadotTheme.spacings.medium)) {
                PolkadotSearchField(
                    modifier = Modifier.fillMaxWidth(),
                    value = "Input text",
                    onValueChange = {},
                    onClear = {},
                    placeholder = "Search",
                )
            }
            Row(modifier = Modifier.padding(PolkadotTheme.spacings.medium)) {
                PolkadotSearchField(
                    modifier = Modifier.fillMaxWidth(),
                    value = "",
                    onValueChange = {},
                    onClear = {},
                    placeholder = "Search",
                )
            }
        }
    }
}
