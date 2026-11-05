package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardSize
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview.PocketFacePreviewContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview.PocketFacePreviewState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PocketFacePreviewScreen(contract: PocketFacePreviewContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    PocketFacePreviewScreenInternal(
        state = state,
        imageResolver = contract.imageResolver,
        backImageResolver = contract.backImageResolver,
        onBackClick = contract::onBackClick,
        onUrlChanged = contract::onUrlChanged,
        onBackUrlChanged = contract::onBackUrlChanged,
        onDrawClick = contract::onDrawClick,
    )
}

@Composable
private fun PocketFacePreviewScreenInternal(
    state: PocketFacePreviewState,
    imageResolver: JsImageResolver,
    backImageResolver: JsImageResolver,
    onBackClick: () -> Unit,
    onUrlChanged: (String) -> Unit,
    onBackUrlChanged: (String) -> Unit,
    onDrawClick: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.pocket_face_preview_title),
                navigationAction = rememberTopBarAction(action = onBackClick),
            )

            Column(
                modifier = Modifier
                    .verticalScroll(rememberScrollState())
                    .padding(PolkadotTheme.spacings.large)
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    NovaTextField(
                        modifier = Modifier.weight(1f),
                        value = state.url,
                        onValueChange = onUrlChanged,
                        placeholder = {
                            NovaText(
                                text = stringResource(RCommon.string.pocket_face_preview_url_hint),
                                style = PolkadotTheme.typography.body.large,
                                color = PolkadotTheme.colors.fg.tertiary,
                            )
                        }
                    )

                    HorizontalSpacer { medium }

                    PolkadotTextButton(
                        text = stringResource(RCommon.string.pocket_face_preview_draw),
                        onClick = onDrawClick,
                        enabled = state.url.isNotBlank(),
                    )
                }

                VerticalSpacer { medium }

                NovaTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = state.backUrl,
                    onValueChange = onBackUrlChanged,
                    placeholder = {
                        NovaText(
                            text = stringResource(RCommon.string.pocket_face_preview_back_url_hint),
                            style = PolkadotTheme.typography.body.large,
                            color = PolkadotTheme.colors.fg.tertiary,
                        )
                    }
                )

                VerticalSpacer { large }

                key(state.draws) {
                    CardFrame(face = state.face, imageResolver = imageResolver)
                    state.back?.let { back ->
                        VerticalSpacer { large }
                        CardFrame(face = back, imageResolver = backImageResolver)
                    }
                }
            }
        }
    }
}

/** The same frame the approval sheet gives a card, so what is drawn here is the size and shape the user will see. */
@Composable
private fun CardFrame(face: LoadingState<JsWidget>?, imageResolver: JsImageResolver) {
    PocketCardTheme {
        PolkadotSurface(
            modifier = Modifier
                .fillMaxWidth()
                .height(PocketCardSize.HEIGHT),
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.bg.surface.container,
        ) {
            CompositionLocalProvider(LocalJsImageResolver provides imageResolver) {
                FaceOrExplanation(face = face)
            }
        }
    }
}

@Composable
private fun FaceOrExplanation(face: LoadingState<JsWidget>?) {
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.Center,
    ) {
        when (face) {
            null -> Explanation(stringResource(RCommon.string.pocket_face_preview_empty))

            LoadingState.Loading -> NovaCircularProgressIndicator()

            // The decoder's own message names what it refused, which is the useful half of a failure.
            // Some failures carry none at all, and the type is then the only thing left to go on.
            is LoadingState.Error -> Explanation(
                face.exception.message ?: face.exception.javaClass.simpleName
            )

            is LoadingState.Loaded -> JsWidgetRenderer(
                widget = face.data,
                modifier = Modifier.fillMaxSize(),
                jsEventHandler = { _, _ -> },
            )
        }
    }
}

@Composable
private fun Explanation(text: String) {
    NovaText(
        modifier = Modifier.padding(PolkadotTheme.spacings.large),
        text = text,
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.secondary,
        textAlign = TextAlign.Center,
    )
}

@Preview
@Composable
private fun PocketFacePreviewScreenPreview() {
    PolkadotTheme {
        PocketFacePreviewScreenInternal(
            state = PocketFacePreviewState(
                url = "http://127.0.0.1:5173/faces/loyalty.json",
                backUrl = "http://127.0.0.1:5173/faces/loyalty-back.json",
                face = LoadingState.Loaded(JsWidget.Text(text = "Loyalty")),
                back = LoadingState.Loaded(JsWidget.Text(text = "Back")),
                draws = 1,
            ),
            imageResolver = JsImageResolver { null },
            backImageResolver = JsImageResolver { null },
            onBackClick = {},
            onUrlChanged = {},
            onBackUrlChanged = {},
            onDrawClick = {},
        )
    }
}
