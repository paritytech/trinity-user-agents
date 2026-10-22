package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUp
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.PolkadotInputField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.ChatTestTags

@Composable
internal fun ChatInputField(
    modifier: Modifier,
    text: String,
    characterLimit: Int?,
    onTextChanged: (String) -> Unit,
    onSendAction: () -> Unit,
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.Bottom,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            MessageTextField(text = text, onTextChanged = onTextChanged)

            if (characterLimit != null) {
                CharacterCounter(
                    modifier = Modifier
                        .align(Alignment.End)
                        .padding(end = PolkadotTheme.spacings.small, bottom = PolkadotTheme.spacings.small),
                    count = text.length,
                    limit = characterLimit,
                )
            }
        }

        AnimatedVisibility(
            modifier = Modifier.align(Alignment.Bottom),
            visible = text.isNotEmpty(),
            enter = scaleIn(),
            exit = scaleOut()
        ) {
            PolkadotIconButton(
                modifier = Modifier
                    .padding(PolkadotTheme.spacings.small)
                    .testTag(ChatTestTags.CHAT_SEND_BUTTON),
                icon = NovaIcons.ArrowUp,
                onClick = onSendAction,
                style = PolkadotButtonStyle.primary(),
                size = PolkadotIconButtonSize.extraSmall(),
                shape = PolkadotTheme.shapes.full,
            )
        }
    }
}

@Composable
private fun MessageTextField(
    text: String,
    onTextChanged: (String) -> Unit,
) {
    PolkadotInputField(
        modifier = Modifier
            .testTag(ChatTestTags.CHAT_MESSAGE_INPUT)
            .fillMaxWidth()
            .padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                top = PolkadotTheme.spacings.extraMedium,
                bottom = PolkadotTheme.spacings.extraMedium,
            ),
        value = text,
        onValueChange = onTextChanged,
        singleLine = false,
        maxLines = 6,
        textStyle = PolkadotTheme.typography.paragraph.large,
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Sentences,
            autoCorrectEnabled = true
        ),
        placeholder = {
            NovaText(stringResource(R.string.chat_details_input_field_placeholder))
        },
        contentPadding = PaddingValues(PolkadotTheme.spacings.zero),
    )
}

@Composable
private fun CharacterCounter(
    modifier: Modifier,
    count: Int,
    limit: Int,
) {
    NovaText(
        modifier = modifier,
        text = stringResource(R.string.chat_input_character_counter, count, limit),
        style = PolkadotTheme.typography.caption.medium,
        color = PolkadotTheme.colors.fg.tertiary,
    )
}

@Preview
@Composable
private fun InputFieldPreview() {
    PolkadotTheme {
        var t by remember { mutableStateOf("sdas") }
        ChatInputField(
            modifier = Modifier.fillMaxWidth(),
            text = t,
            characterLimit = null,
            onTextChanged = { t = it },
            onSendAction = {}
        )
    }
}

@Preview
@Composable
private fun InputFieldWithCharacterCounterPreview() {
    PolkadotTheme {
        var t by remember { mutableStateOf("Hi! I'd like to add you as a contact") }
        ChatInputField(
            modifier = Modifier.fillMaxWidth(),
            text = t,
            characterLimit = 1000,
            onTextChanged = { t = it },
            onSendAction = {}
        )
    }
}
