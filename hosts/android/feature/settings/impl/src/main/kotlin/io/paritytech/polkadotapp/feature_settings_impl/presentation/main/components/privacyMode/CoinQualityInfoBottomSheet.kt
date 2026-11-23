@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withSpanStyle
import io.paritytech.polkadotapp.common.R as RCommon

private enum class CoinQualityInfoPage { WHAT_IS_COIN_QUALITY, WHY_IT_MATTERS }

@Composable
internal fun CoinQualityInfoBottomSheet(
    isVisible: Boolean,
    onDismissRequest: () -> Unit
) {
    NovaModalBottomSheet(
        isVisible = isVisible,
        onDismissRequest = onDismissRequest
    ) {
        var page by remember { mutableStateOf(CoinQualityInfoPage.WHAT_IS_COIN_QUALITY) }

        BackHandler(enabled = page == CoinQualityInfoPage.WHY_IT_MATTERS) {
            page = CoinQualityInfoPage.WHAT_IS_COIN_QUALITY
        }

        when (page) {
            CoinQualityInfoPage.WHAT_IS_COIN_QUALITY -> WhatIsCoinQualityContent(
                onGotItClick = onDismissRequest,
                onWhyItMattersClick = { page = CoinQualityInfoPage.WHY_IT_MATTERS }
            )

            CoinQualityInfoPage.WHY_IT_MATTERS -> WhyItMattersContent(
                onBackClick = { page = CoinQualityInfoPage.WHAT_IS_COIN_QUALITY },
                onGotItClick = onDismissRequest
            )
        }
    }
}

@Composable
private fun WhatIsCoinQualityContent(
    onGotItClick: () -> Unit,
    onWhyItMattersClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = PolkadotTheme.spacings.small)
    ) {
        ScrollableSheetContent {
            SheetTitle(text = stringResource(RCommon.string.payment_privacy_mode_coin_quality_title))

            Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium)) {
                Image(
                    modifier = Modifier
                        .padding(horizontal = PolkadotTheme.spacings.large)
                        .fillMaxWidth()
                        .clip(PolkadotTheme.shapes.mediumIncreased),
                    painter = painterResource(RCommon.drawable.img_coin_quality),
                    contentDescription = null,
                    contentScale = ContentScale.FillWidth
                )

                SheetBody(
                    text = highlightedStringResource(
                        RCommon.string.payment_privacy_mode_coin_quality_description,
                        RCommon.string.payment_privacy_mode_coin_quality_highlight_coin_quality,
                        RCommon.string.payment_privacy_mode_coin_quality_highlight_indistinguishable,
                        RCommon.string.payment_privacy_mode_coin_quality_highlight_higher_quality
                    )
                )
            }
        }

        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_got_it),
                style = PolkadotButtonStyle.primary(),
                size = PolkadotButtonSize.large(),
                shape = PolkadotButtonShape.pill,
                onClick = onGotItClick
            )

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.payment_privacy_mode_why_it_matters_title),
                style = PolkadotButtonStyle.tertiary(),
                size = PolkadotButtonSize.large(),
                shape = PolkadotButtonShape.pill,
                onClick = onWhyItMattersClick
            )
        }
    }
}

@Composable
private fun WhyItMattersContent(
    onBackClick: () -> Unit,
    onGotItClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = PolkadotTheme.spacings.small)
    ) {
        PolkadotIconButton(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.small
            ),
            icon = NovaIcons.ArrowLeft,
            onClick = onBackClick,
            style = PolkadotButtonStyle.ghost(),
            size = PolkadotIconButtonSize.mediumIncreased(),
            shape = PolkadotButtonShape.pill
        )

        ScrollableSheetContent {
            SheetTitle(text = stringResource(RCommon.string.payment_privacy_mode_why_it_matters_title))

            SheetBody(
                text = highlightedStringResource(
                    RCommon.string.payment_privacy_mode_why_it_matters_description,
                    RCommon.string.payment_privacy_mode_why_it_matters_highlight_same_denomination,
                    RCommon.string.payment_privacy_mode_coin_quality_highlight_indistinguishable,
                    RCommon.string.payment_privacy_mode_why_it_matters_highlight_fungibility
                )
            )
        }

        PolkadotTextButton(
            modifier = Modifier
                .padding(PolkadotTheme.spacings.mediumIncreased)
                .fillMaxWidth(),
            text = stringResource(RCommon.string.common_got_it),
            style = PolkadotButtonStyle.tertiary(),
            size = PolkadotButtonSize.large(),
            shape = PolkadotButtonShape.pill,
            onClick = onGotItClick
        )
    }
}

@Composable
private fun ColumnScope.ScrollableSheetContent(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .weight(1f, fill = false)
            .verticalScroll(rememberScrollState()),
        content = content
    )
}

@Composable
private fun SheetTitle(text: String) {
    NovaText(
        modifier = Modifier.padding(
            horizontal = PolkadotTheme.spacings.large,
            vertical = PolkadotTheme.spacings.mediumIncreased
        ),
        text = text,
        style = PolkadotTheme.typography.headline.small,
        color = PolkadotTheme.colors.fg.primary
    )
}

@Composable
private fun SheetBody(text: AnnotatedString) {
    NovaText(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large),
        text = text,
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.secondary
    )
}

@Composable
private fun highlightedStringResource(
    @StringRes textRes: Int,
    @StringRes vararg highlightRes: Int
): AnnotatedString {
    val highlights = highlightRes.map { stringResource(it) }.toTypedArray()

    return stringResource(textRes, *highlights)
        .withSpanStyle(SpanStyle(color = PolkadotTheme.colors.fg.primary), *highlights)
}

@Preview
@Composable
private fun WhatIsCoinQualityContentPreview() {
    PolkadotTheme {
        NovaBottomSheetSurface {
            WhatIsCoinQualityContent(onGotItClick = {}, onWhyItMattersClick = {})
        }
    }
}

@Preview
@Composable
private fun WhyItMattersContentPreview() {
    PolkadotTheme {
        NovaBottomSheetSurface {
            WhyItMattersContent(onBackClick = {}, onGotItClick = {})
        }
    }
}
