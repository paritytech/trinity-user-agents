package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardSize
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard.PocketAddCardContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard.PocketAddCardTestTags
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard.PocketAddCardUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PocketAddCardScreen(contract: PocketAddCardContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    PocketAddCardScreenInternal(
        state = state,
        onAddClicked = contract::onAddClicked,
        onCancelClicked = contract::onCancelClicked,
    )
}

@Composable
private fun PocketAddCardScreenInternal(
    state: LoadingState<PocketAddCardUiState>,
    onAddClicked: () -> Unit,
    onCancelClicked: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased)
                .testTag(PocketAddCardTestTags.SHEET),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            NovaText(
                text = stringResource(RCommon.string.pocket_add_card_title),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center,
            )

            VerticalSpacer { mediumIncreased }

            when (state) {
                is LoadingState.Loading -> LoadingScreenState(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(PocketCardSize.HEIGHT),
                )

                is LoadingState.Error -> NovaText(
                    text = stringResource(RCommon.string.pocket_add_card_failed),
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.secondary,
                    textAlign = TextAlign.Center,
                )

                is LoadingState.Loaded -> OfferContent(state.data)
            }

            VerticalSpacer { extraLarge }

            Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                PolkadotTextButton(
                    modifier = Modifier
                        .weight(1f)
                        .testTag(PocketAddCardTestTags.CANCEL_BUTTON),
                    text = stringResource(RCommon.string.pocket_add_card_cancel),
                    style = PolkadotButtonStyle.ghost(),
                    onClick = onCancelClicked,
                )

                PolkadotTextButton(
                    modifier = Modifier
                        .weight(1f)
                        .testTag(PocketAddCardTestTags.ADD_BUTTON),
                    text = stringResource(RCommon.string.pocket_add_card_action),
                    enabled = state is LoadingState.Loaded && !state.data.adding,
                    loading = state is LoadingState.Loaded && state.data.adding,
                    onClick = onAddClicked,
                )
            }
        }
    }
}

@Composable
private fun OfferContent(offer: PocketAddCardUiState) {
    NovaText(
        text = stringResource(RCommon.string.pocket_add_card_subtitle, offer.productName),
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.secondary,
        textAlign = TextAlign.Center,
    )

    VerticalSpacer { mediumIncreased }

    PocketCardTheme {
        PolkadotSurface(
            modifier = Modifier
                .fillMaxWidth()
                .height(PocketCardSize.HEIGHT)
                .testTag(PocketAddCardTestTags.PREVIEW),
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.bg.surface.container,
        ) {
            // The face is product-authored and inert here: the card is not in the collection yet.
            CompositionLocalProvider(LocalJsImageResolver provides offer.imageResolver) {
                JsWidgetRenderer(widget = offer.face, jsEventHandler = { _, _ -> })
            }
        }
    }

    VerticalSpacer { small }

    NovaText(
        text = offer.title,
        style = PolkadotTheme.typography.title.medium,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center,
    )
}

@Preview
@Composable
private fun PocketAddCardPreview() {
    PolkadotTheme {
        PocketAddCardScreenInternal(
            state = LoadingState.Loaded(
                PocketAddCardUiState(
                    productName = "Coinflip",
                    title = "Loyalty",
                    face = JsWidget.Text(text = "Loyalty", style = JsTypographyStyle.HEADLINE_LARGE),
                    imageResolver = JsImageResolver { null },
                    adding = false,
                ),
            ),
            onAddClicked = {},
            onCancelClicked = {},
        )
    }
}
