package io.paritytech.polkadotapp.feature_usernames_api.presentation.compose

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_api.presentation.ClaimUsernameTestTags
import io.paritytech.polkadotapp.feature_usernames_api.presentation.MIN_USERNAME_LENGTH
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ClaimButton(
    username: String,
    showClearAction: Boolean,
    claimButtonEnabled: Boolean,
    isClaimingInProgress: Boolean,
    onClaimAction: () -> Unit,
    onClearAction: () -> Unit
) {
    val modifier = Modifier
        .fillMaxWidth()
        .padding(horizontal = PolkadotTheme.spacings.large)
        .testTag(ClaimUsernameTestTags.USERNAME_SUBMIT_BUTTON)

    when {
        username.length < MIN_USERNAME_LENGTH -> PolkadotTextButton(
            modifier = modifier,
            text = stringResource(RCommon.string.pick_username_action_too_short, MIN_USERNAME_LENGTH),
            style = PolkadotButtonStyle.secondary(),
            enabled = false,
            onClick = {}
        )

        showClearAction -> PolkadotTextButton(
            modifier = modifier,
            text = stringResource(RCommon.string.pick_username_action_clear),
            style = PolkadotButtonStyle.secondary(),
            onClick = onClearAction
        )

        else -> PolkadotTextButton(
            modifier = modifier,
            text = stringResource(RCommon.string.pick_username_action_claim),
            style = PolkadotButtonStyle.primary(),
            enabled = claimButtonEnabled,
            loading = isClaimingInProgress,
            onClick = onClaimAction
        )
    }
}
