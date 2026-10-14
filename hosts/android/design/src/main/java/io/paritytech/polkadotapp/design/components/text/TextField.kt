@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.design.components.text

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.VisualTransformation
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Deprecated(
    message = "Use PolkadotInputField; pass a bordered surface into its `container` slot.",
)
@Composable
fun NovaTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    singleLine: Boolean = true,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    label: @Composable (() -> Unit)? = null,
    placeholder: @Composable (() -> Unit)? = null,
    suffix: @Composable (() -> Unit)? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    containerColor: Color = PolkadotTheme.colors.bg.surface.container,
    border: BorderStroke? = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary)
) {
    PolkadotInputField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        enabled = enabled,
        readOnly = readOnly,
        singleLine = singleLine,
        maxLines = maxLines,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        label = label,
        placeholder = placeholder,
        leadingIcon = leadingIcon,
        suffix = suffix,
        container = {
            PolkadotSurface(
                shape = PolkadotTheme.shapes.medium,
                color = containerColor,
                border = border,
                content = {}
            )
        },
        contentPadding = PaddingValues(
            start = PolkadotTheme.spacings.mediumIncreased,
            end = PolkadotTheme.spacings.small,
            top = PolkadotTheme.spacings.small,
            bottom = PolkadotTheme.spacings.mediumIncreased
        ),
    )
}
