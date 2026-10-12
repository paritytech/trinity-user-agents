package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.TrUAPIContactPickContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.TrUAPIContactPickRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.TrUAPIContactPickUiState
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun TrUAPIContactPickScreen(contract: TrUAPIContactPickContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { contract.onShown() }

    state.onLoaded { data ->
        TrUAPIContactPickScreenInternal(
            state = data,
            onPick = contract::onContactClicked,
            onDismiss = contract::onDismissClicked,
        )
    }
}

@Composable
private fun TrUAPIContactPickScreenInternal(
    state: TrUAPIContactPickUiState,
    onPick: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier.padding(
                top = PolkadotTheme.spacings.large,
                bottom = PolkadotTheme.spacings.mediumIncreased,
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
            ),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            NovaText(
                text = stringResource(RCommon.string.truapi_contact_pick_title),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
            )

            VerticalSpacer { small }

            NovaText(
                text = stringResource(RCommon.string.truapi_contact_pick_body, state.productId),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.Center,
            )

            VerticalSpacer { mediumIncreased }

            state.contacts.forEach { contact ->
                PolkadotTextButton(
                    modifier = Modifier.fillMaxWidth(),
                    text = contact.name
                        ?: stringResource(RCommon.string.truapi_contact_pick_unnamed),
                    style = PolkadotButtonStyle.secondary(),
                    onClick = { onPick(contact.index) },
                )

                VerticalSpacer { small }
            }

            VerticalSpacer { small }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.truapi_contact_pick_dismiss),
                style = PolkadotButtonStyle.ghost(),
                onClick = onDismiss,
            )
        }
    }
}

@Preview
@Composable
private fun TrUAPIContactPickScreenPreview() {
    PolkadotTheme {
        TrUAPIContactPickScreenInternal(
            state = TrUAPIContactPickUiState(
                productId = "browse.dot",
                contacts = persistentListOf(
                    TrUAPIContactPickRow(index = 0, name = "alice"),
                    TrUAPIContactPickRow(index = 1, name = null),
                ),
            ),
            onPick = {},
            onDismiss = {},
        )
    }
}
