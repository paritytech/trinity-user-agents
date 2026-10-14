@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.design.components.text

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotInputField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    singleLine: Boolean = true,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    textStyle: TextStyle = PolkadotTheme.typography.body.large,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    label: @Composable (() -> Unit)? = null,
    placeholder: @Composable (() -> Unit)? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    suffix: @Composable (() -> Unit)? = null,
    container: @Composable () -> Unit = {},
    contentPadding: PaddingValues = PaddingValues(0.dp),
) {
    BasicTextField(
        modifier = modifier,
        value = value,
        onValueChange = onValueChange,
        enabled = enabled,
        readOnly = readOnly,
        singleLine = singleLine,
        maxLines = maxLines,
        cursorBrush = SolidColor(PolkadotTheme.colors.fg.primary),
        textStyle = if (textStyle.color.isSpecified) {
            textStyle
        } else {
            textStyle.copy(color = PolkadotTheme.colors.fg.primary)
        },
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        decorationBox = { innerTextField ->
            TextFieldDefaults.DecorationBox(
                value = value,
                innerTextField = innerTextField,
                enabled = enabled,
                singleLine = singleLine,
                visualTransformation = VisualTransformation.None,
                interactionSource = remember { MutableInteractionSource() },
                container = container,
                contentPadding = contentPadding,
                label = label,
                placeholder = placeholder,
                leadingIcon = leadingIcon,
                suffix = suffix,
                colors = TextFieldDefaults.colors(
                    focusedPlaceholderColor = PolkadotTheme.colors.fg.disabled,
                    unfocusedPlaceholderColor = PolkadotTheme.colors.fg.disabled,
                    disabledPlaceholderColor = PolkadotTheme.colors.fg.disabled,
                ),
            )
        },
    )
}
