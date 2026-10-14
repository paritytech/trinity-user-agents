package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.compose

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.FillerSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_usernames_api.presentation.ClaimUsernameTestTags
import io.paritytech.polkadotapp.feature_usernames_api.presentation.compose.ClaimButton
import io.paritytech.polkadotapp.feature_usernames_api.presentation.compose.UsernameTextField
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.DigitsFieldState
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.UsernameFieldStatus
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.UsernameFieldStyle
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.ClaimUsernameContract
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.ClaimUsernameFieldState
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.ClaimUsernameProgress
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.toPresentationError
import io.paritytech.polkadotapp.common.R as RCommon

private data class ClaimUsernameTexts(
    val header: String,
    val description: String
)

@Composable
private fun buildRecoverHereText(): AnnotatedString {
    val recoverHereText = stringResource(RCommon.string.claim_username_recover_here)
    val fullText = stringResource(RCommon.string.claim_username_already_have_account, recoverHereText)

    return buildAnnotatedString {
        withStyle(SpanStyle(color = PolkadotTheme.colors.fg.tertiary)) {
            append(fullText)
        }
        addStyle(
            SpanStyle(color = PolkadotTheme.colors.fg.primary),
            fullText.indexOf(recoverHereText),
            fullText.length
        )
    }
}

private const val TERMS_LINK_TAG = "terms"
private const val PRIVACY_POLICY_LINK_TAG = "privacy_policy"

@Composable
private fun buildTermsPrivacyText(
    onTermsClicked: () -> Unit,
    onPrivacyPolicyClicked: () -> Unit
): AnnotatedString {
    val terms = stringResource(RCommon.string.claim_username_terms)
    val privacyPolicy = stringResource(RCommon.string.claim_username_privacy_policy)
    val fullText = stringResource(RCommon.string.claim_username_terms_privacy_consent, terms, privacyPolicy)

    val linkStyles = TextLinkStyles(style = SpanStyle(color = PolkadotTheme.colors.fg.primary))

    return buildAnnotatedString {
        withStyle(SpanStyle(color = PolkadotTheme.colors.fg.tertiary)) {
            append(fullText)
        }

        val termsStart = fullText.indexOf(terms)
        addLink(
            LinkAnnotation.Clickable(
                tag = TERMS_LINK_TAG,
                styles = linkStyles,
                linkInteractionListener = { onTermsClicked() }
            ),
            termsStart,
            termsStart + terms.length
        )

        val privacyStart = fullText.indexOf(privacyPolicy)
        addLink(
            LinkAnnotation.Clickable(
                tag = PRIVACY_POLICY_LINK_TAG,
                styles = linkStyles,
                linkInteractionListener = { onPrivacyPolicyClicked() }
            ),
            privacyStart,
            privacyStart + privacyPolicy.length
        )
    }
}

@Composable
private fun resolveClaimUsernameTexts(showRecoverOption: Boolean): ClaimUsernameTexts {
    return if (showRecoverOption) {
        ClaimUsernameTexts(
            header = stringResource(RCommon.string.claim_username_welcome_header),
            description = stringResource(RCommon.string.claim_username_onboarding_description)
        )
    } else {
        ClaimUsernameTexts(
            header = stringResource(RCommon.string.claim_username_recovered_header),
            description = stringResource(RCommon.string.claim_username_recovered_description)
        )
    }
}

@Composable
fun PickUsernameScreen(contract: ClaimUsernameContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    contract.messageEvents.collectAsEffect { context, resId ->
        Toast.makeText(context, context.getString(resId), Toast.LENGTH_SHORT).show()
    }

    when (state.progress) {
        ClaimUsernameProgress.CREATING,
        ClaimUsernameProgress.RECOVERING -> FullScreenLoadingContent(state.progress)
        ClaimUsernameProgress.NONE,
        ClaimUsernameProgress.CLAIMING -> PickUsernameScreenInternal(
            username = state.username,
            onUsernameChanged = contract::onUsernameChanged,
            onDigitsChanged = contract::onDigitsChanged,
            fieldState = state.fieldState,
            digitsFieldState = state.digitsFieldState,
            claimButtonEnabled = state.claimButtonEnabled,
            isClaimingInProgress = state.progress == ClaimUsernameProgress.CLAIMING,
            showRecoverOption = state.showRecoverOption,
            onClaimAction = contract::onClaimClicked,
            onClearAction = contract::onClearAction,
            onRecoverAction = contract::onRecoverClicked,
            onTermsClicked = contract::onTermsClicked,
            onPrivacyPolicyClicked = contract::onPrivacyPolicyClicked
        )
    }
}

@Composable
private fun FullScreenLoadingContent(progress: ClaimUsernameProgress) {
    val message = when (progress) {
        ClaimUsernameProgress.CREATING -> stringResource(RCommon.string.claim_username_creating_account)
        ClaimUsernameProgress.RECOVERING -> stringResource(RCommon.string.login_restoring_account_progress)
        else -> ""
    }

    PolkadotSurface {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(PolkadotTheme.colors.bg.surface.main),
            contentAlignment = Alignment.Center
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
            ) {
                NovaCircularProgressIndicator(
                    color = PolkadotTheme.colors.fg.primary
                )
                NovaText(
                    text = message,
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.secondary
                )
            }
        }
    }
}

@Composable
private fun PickUsernameScreenInternal(
    username: String,
    onUsernameChanged: (String) -> Unit,
    onDigitsChanged: (String) -> Unit,
    fieldState: ClaimUsernameFieldState,
    digitsFieldState: DigitsFieldState,
    claimButtonEnabled: Boolean,
    isClaimingInProgress: Boolean,
    showRecoverOption: Boolean,
    onClaimAction: () -> Unit,
    onClearAction: () -> Unit,
    onRecoverAction: () -> Unit,
    onTermsClicked: () -> Unit,
    onPrivacyPolicyClicked: () -> Unit,
) {
    val texts = resolveClaimUsernameTexts(showRecoverOption)

    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .systemBarsPadding()
                .imePadding(),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            PolkadotTopBar(
                title = texts.header,
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { mediumIncreased }

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = stringResource(RCommon.string.pick_username_title),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { extraMedium }

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = texts.description,
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { extraLarge }

            UsernameTextField(
                username = username,
                postfix = null,
                onUsernameChanged = onUsernameChanged,
                onDigitsChanged = onDigitsChanged,
                status = claimFieldStatus(fieldState, digitsFieldState),
                digitsFieldState = digitsFieldState,
                isEnabled = isClaimingInProgress.not()
            )

            FillerSpacer()

            if (showRecoverOption) {
                PolkadotSurface(
                    modifier = Modifier.testTag(ClaimUsernameTestTags.ONBOARDING_RECOVER_HERE),
                    onClick = onRecoverAction
                ) {
                    NovaText(
                        text = buildRecoverHereText(),
                        style = PolkadotTheme.typography.body.medium,
                    )
                }

                VerticalSpacer { large }
            }

            ClaimButton(
                username = username,
                showClearAction = fieldState is ClaimUsernameFieldState.Taken,
                claimButtonEnabled = claimButtonEnabled,
                isClaimingInProgress = isClaimingInProgress,
                onClaimAction = onClaimAction,
                onClearAction = onClearAction
            )

            VerticalSpacer { large }

            if (showRecoverOption) {
                NovaText(
                    modifier = Modifier
                        .padding(
                            horizontal = PolkadotTheme.spacings.extraLargeIncreased
                        ),
                    text = buildTermsPrivacyText(
                        onTermsClicked = onTermsClicked,
                        onPrivacyPolicyClicked = onPrivacyPolicyClicked
                    ),
                    style = PolkadotTheme.typography.body.medium,
                    textAlign = TextAlign.Center
                )

                VerticalSpacer { large }
            }
        }
    }
}

@Preview
@Composable
fun PickUsernameScreenPreview() {
    PolkadotTheme {
        PickUsernameScreenInternal(
            username = "",
            onUsernameChanged = {},
            onDigitsChanged = {},
            fieldState = ClaimUsernameFieldState.Neutral,
            digitsFieldState = DigitsFieldState.Hidden,
            claimButtonEnabled = false,
            isClaimingInProgress = false,
            showRecoverOption = true,
            onClaimAction = {},
            onClearAction = {},
            onRecoverAction = {},
            onTermsClicked = {},
            onPrivacyPolicyClicked = {}
        )
    }
}

@Preview
@Composable
fun PickUsernameAvailableScreenPreview() {
    PolkadotTheme {
        PickUsernameScreenInternal(
            username = "white_paper",
            onUsernameChanged = {},
            onDigitsChanged = {},
            fieldState = ClaimUsernameFieldState.Available,
            digitsFieldState = DigitsFieldState.Visible(digits = "92", isValid = true),
            claimButtonEnabled = true,
            isClaimingInProgress = false,
            showRecoverOption = true,
            onClaimAction = {},
            onClearAction = {},
            onRecoverAction = {},
            onTermsClicked = {},
            onPrivacyPolicyClicked = {}
        )
    }
}

@Preview
@Composable
private fun PickUsernameRecoveredScreenPreview() {
    PolkadotTheme {
        PickUsernameScreenInternal(
            username = "",
            onUsernameChanged = {},
            onDigitsChanged = {},
            fieldState = ClaimUsernameFieldState.Neutral,
            digitsFieldState = DigitsFieldState.Hidden,
            claimButtonEnabled = false,
            isClaimingInProgress = false,
            showRecoverOption = false,
            onClaimAction = {},
            onClearAction = {},
            onRecoverAction = {},
            onTermsClicked = {},
            onPrivacyPolicyClicked = {}
        )
    }
}

@Preview
@Composable
private fun FullScreenLoadingPreview() {
    PolkadotTheme {
        FullScreenLoadingContent(ClaimUsernameProgress.CREATING)
    }
}

@Composable
private fun claimFieldStatus(
    fieldState: ClaimUsernameFieldState,
    digitsFieldState: DigitsFieldState,
): UsernameFieldStatus {
    if (digitsFieldState is DigitsFieldState.Visible && !digitsFieldState.isValid) {
        return UsernameFieldStatus(
            style = UsernameFieldStyle.Error,
            text = stringResource(RCommon.string.pick_username_digits_taken),
        )
    }

    return when (fieldState) {
        ClaimUsernameFieldState.Neutral -> UsernameFieldStatus(UsernameFieldStyle.Neutral, null)

        ClaimUsernameFieldState.Taken -> UsernameFieldStatus(
            style = UsernameFieldStyle.Error,
            text = stringResource(RCommon.string.pick_username_state_taken),
        )

        ClaimUsernameFieldState.Invalid -> UsernameFieldStatus(
            style = UsernameFieldStyle.Error,
            text = stringResource(RCommon.string.pick_username_state_invalid),
        )

        ClaimUsernameFieldState.Available -> UsernameFieldStatus(
            style = UsernameFieldStyle.Success,
            text = stringResource(RCommon.string.pick_username_state_available),
        )

        is ClaimUsernameFieldState.Error -> fieldState.error.toStatus()
    }
}

@Composable
private fun UsernameFlowError.toStatus(): UsernameFieldStatus = when (this) {
    // The ViewModel never stores this; rendering nothing is the right fallback if it ever does.
    UsernameFlowError.Cancelled -> UsernameFieldStatus(UsernameFieldStyle.Neutral, null)

    else -> UsernameFieldStatus(UsernameFieldStyle.Error, toPresentationError().message())
}
