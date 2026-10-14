package io.paritytech.polkadotapp.feature_usernames_api.presentation.compose

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_api.presentation.ClaimUsernameTestTags
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.DigitsFieldState
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.UsernameFieldStatus
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.UsernameFieldStyle

@Composable
fun UsernameTextField(
    username: String,
    postfix: String?,
    onUsernameChanged: (String) -> Unit,
    onDigitsChanged: (String) -> Unit,
    status: UsernameFieldStatus,
    digitsFieldState: DigitsFieldState,
    isEnabled: Boolean,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaTextField(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.large)
                .testTag(ClaimUsernameTestTags.USERNAME_INPUT),
            value = username,
            onValueChange = onUsernameChanged,
            suffix = {
                when (digitsFieldState) {
                    is DigitsFieldState.Visible -> DigitsSuffix(
                        digits = digitsFieldState.digits,
                        onDigitsChanged = onDigitsChanged,
                        isEnabled = isEnabled
                    )

                    is DigitsFieldState.Hidden -> {
                        if (postfix != null) {
                            Row {
                                NovaText(text = ".$postfix")
                                HorizontalSpacer { small }
                            }
                        }
                    }
                }
            },
            border = BorderStroke(width = 1.dp, color = statusColor(status.style)),
            enabled = isEnabled
        )

        AnimatedVisibility(
            modifier = Modifier.fillMaxWidth(),
            visible = status.text != null
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                VerticalSpacer { small }

                Surface(
                    modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large),
                    shape = PolkadotTheme.shapes.small,
                    color = Color(0x1FFFFFFF)
                ) {
                    NovaText(
                        modifier = Modifier
                            .padding(
                                vertical = PolkadotTheme.spacings.tiny,
                                horizontal = PolkadotTheme.spacings.small
                            ),
                        textAlign = TextAlign.Center,
                        style = PolkadotTheme.typography.body.small,
                        text = status.text.orEmpty(),
                        color = statusColor(status.style)
                    )
                }
            }
        }
    }
}

@Composable
private fun DigitsSuffix(
    digits: String,
    onDigitsChanged: (String) -> Unit,
    isEnabled: Boolean,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        NovaText(
            text = ".",
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.tertiary
        )
        BasicTextField(
            modifier = Modifier
                .width(IntrinsicSize.Min)
                .widthIn(min = 28.dp)
                .testTag(ClaimUsernameTestTags.DIGITS_INPUT),
            value = digits,
            onValueChange = onDigitsChanged,
            textStyle = PolkadotTheme.typography.title.large.copy(
                color = PolkadotTheme.colors.fg.primary
            ),
            cursorBrush = SolidColor(PolkadotTheme.colors.fg.primary),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            singleLine = true,
            enabled = isEnabled
        )
        HorizontalSpacer { small }
    }
}

@Composable
private fun statusColor(style: UsernameFieldStyle) = when (style) {
    UsernameFieldStyle.Neutral -> PolkadotTheme.colors.stroke.primary
    UsernameFieldStyle.Error -> PolkadotTheme.colors.fg.error
    UsernameFieldStyle.Success -> PolkadotTheme.colors.fg.success
}

@Preview
@Composable
fun PickUsernameAvailableScreenPreview() {
    PolkadotTheme {
        UsernameTextField(
            username = "best_username",
            onUsernameChanged = {},
            onDigitsChanged = {},
            status = UsernameFieldStatus(UsernameFieldStyle.Success, "It's yours!"),
            digitsFieldState = DigitsFieldState.Visible(digits = "42", isValid = true),
            postfix = null,
            isEnabled = true
        )
    }
}

@Preview
@Composable
fun PickUsernameInvalidDigitsPreview() {
    PolkadotTheme {
        UsernameTextField(
            username = "best_username",
            onUsernameChanged = {},
            onDigitsChanged = {},
            status = UsernameFieldStatus(UsernameFieldStyle.Error, "Digits taken. Try again."),
            digitsFieldState = DigitsFieldState.Visible(digits = "77", isValid = false),
            postfix = null,
            isEnabled = true
        )
    }
}

@Preview
@Composable
fun PickUsernameNeutralScreenPreview() {
    PolkadotTheme {
        UsernameTextField(
            username = "best_username",
            onUsernameChanged = {},
            onDigitsChanged = {},
            status = UsernameFieldStatus(UsernameFieldStyle.Neutral, null),
            digitsFieldState = DigitsFieldState.Hidden,
            postfix = null,
            isEnabled = true
        )
    }
}

@Preview
@Composable
fun PickUsernameTakenScreenPreview() {
    PolkadotTheme {
        UsernameTextField(
            username = "best_username",
            onUsernameChanged = {},
            onDigitsChanged = {},
            status = UsernameFieldStatus(UsernameFieldStyle.Error, "Taken. Try another."),
            digitsFieldState = DigitsFieldState.Hidden,
            postfix = null,
            isEnabled = true
        )
    }
}
