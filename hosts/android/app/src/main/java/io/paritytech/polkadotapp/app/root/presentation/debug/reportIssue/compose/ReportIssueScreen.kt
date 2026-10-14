package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.ISSUE_DESCRIPTION_LIMIT
import io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.ReportIssueContract
import io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.ReportIssueState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.dialog.NovaAlertDialog
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.PolkadotInputField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import java.io.File
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ReportIssueScreen(contract: ReportIssueContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    val keyboard = LocalSoftwareKeyboardController.current

    ReportIssueScreenInternal(
        state = state,
        onDescriptionChanged = contract::onDescriptionChanged,
        onSend = {
            keyboard?.hide()
            contract.onSendClick()
        },
        onClose = contract::onCloseClick,
    )

    if (state.isComplete) {
        NovaAlertDialog(
            text = stringResource(RCommon.string.debug_report_thanks),
            positiveButtonTitle = stringResource(RCommon.string.common_ok),
            onPositiveButtonClick = contract::onCloseClick,
            onDismissRequest = contract::onCloseClick,
        )
    }
}

@Composable
private fun ReportIssueScreenInternal(
    state: ReportIssueState,
    onDescriptionChanged: (String) -> Unit,
    onSend: () -> Unit,
    onClose: () -> Unit,
) {
    val closeDescription = stringResource(RCommon.string.common_close)
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(PolkadotTheme.spacings.large),
        ) {
            Box(modifier = Modifier.fillMaxWidth()) {
                NovaText(
                    modifier = Modifier.align(Alignment.Center),
                    text = stringResource(RCommon.string.debug_report_title),
                    style = PolkadotTheme.typography.title.large,
                )
                PolkadotIconButton(
                    modifier = Modifier.align(Alignment.CenterEnd).semantics { contentDescription = closeDescription },
                    icon = NovaIcons.Close,
                    style = PolkadotButtonStyle.tertiary(),
                    size = PolkadotIconButtonSize.medium(),
                    shape = CircleShape,
                    onClick = onClose,
                )
            }
            VerticalSpacer { large }
            NovaText(
                text = stringResource(RCommon.string.debug_report_what_happened),
                style = PolkadotTheme.typography.title.medium,
            )
            VerticalSpacer { medium }
            PolkadotSurface(
                shape = RoundedCornerShape(24.dp),
                color = PolkadotTheme.colors.bg.action.tertiary,
            ) {
                Column(modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased)) {
                    PolkadotInputField(
                        modifier = Modifier.fillMaxWidth().heightIn(min = 140.dp, max = 220.dp),
                        value = state.description,
                        onValueChange = onDescriptionChanged,
                        enabled = !state.isSending,
                        singleLine = false,
                        placeholder = { NovaText(text = stringResource(RCommon.string.debug_report_placeholder)) },
                    )
                    VerticalSpacer { small }
                    NovaText(
                        modifier = Modifier.align(Alignment.End),
                        text = stringResource(RCommon.string.debug_report_character_count, state.description.length, ISSUE_DESCRIPTION_LIMIT),
                        style = PolkadotTheme.typography.body.small,
                        color = PolkadotTheme.colors.fg.tertiary,
                    )
                }
            }
            VerticalSpacer { mediumIncreased }
            NovaText(
                text = stringResource(RCommon.string.debug_report_attachments),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
            VerticalSpacer { medium }
            PolkadotSurface(shape = RoundedCornerShape(16.dp)) {
                NovaAsyncImage(
                    modifier = Modifier.width(110.dp).height(220.dp),
                    model = File(state.screenshotPath),
                    contentDescription = stringResource(RCommon.string.debug_report_screenshot),
                )
            }
            VerticalSpacer { large }
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_send),
                enabled = state.description.isNotBlank() && !state.isComplete,
                loading = state.isSending,
                onClick = onSend,
            )
        }
    }
}

@Preview
@Composable
private fun ReportIssueScreenPreview() {
    PolkadotTheme {
        ReportIssueScreenInternal(ReportIssueState("", "", false, false), {}, {}, {})
    }
}
