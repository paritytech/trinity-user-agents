package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.mnemonic.ProtectedMnemonic
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic.MnemonicRevealContract
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MnemonicRevealScreen(contract: MnemonicRevealContract) {
    val mnemonic by contract.mnemonic.collectAsStateWithLifecycle()
    val isMnemonicHidden by contract.isMnemonicHidden.collectAsStateWithLifecycle()

    MnemonicRevealScreenInternal(
        onBackAction = contract::back,
        mnemonic = mnemonic,
        isMnemonicHidden = isMnemonicHidden,
        onRevealMnemonic = contract::revealMnemonic
    )
}

@Composable
private fun MnemonicRevealScreenInternal(
    onBackAction: () -> Unit,
    mnemonic: ImmutableList<Word>,
    isMnemonicHidden: Boolean,
    onRevealMnemonic: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .systemBarsPadding(),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(onBackAction),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { small }

            val headerModifier = Modifier.padding(horizontal = 48.dp)

            NovaText(
                modifier = headerModifier,
                text = stringResource(RCommon.string.settings_mnemonic_reveal_title),
                style = PolkadotTheme.typography.headline.small,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { small }

            NovaText(
                modifier = headerModifier,
                text = stringResource(RCommon.string.settings_mnemonic_reveal_description),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { 48.dp }

            ProtectedMnemonic(
                mnemonic = mnemonic,
                onRevealMnemonicAction = onRevealMnemonic,
                isHidden = isMnemonicHidden,
                coverTitle = stringResource(RCommon.string.manual_backup_confirm_reveal_action_title),
                coverDescription = stringResource(RCommon.string.manual_backup_confirm_reveal_action_description)
            )
        }
    }
}

@Preview
@Composable
private fun MnemonicRevealScreenPreview() {
    PolkadotTheme {
        MnemonicRevealScreenInternal(
            onBackAction = {},
            mnemonic = List(12) { Word(it, "word$it") }.toImmutableList(),
            isMnemonicHidden = false,
            onRevealMnemonic = {},
        )
    }
}
