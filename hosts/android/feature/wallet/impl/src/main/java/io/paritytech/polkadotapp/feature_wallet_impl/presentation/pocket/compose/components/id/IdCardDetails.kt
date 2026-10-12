package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.id

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Share
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.IdShareQrCard
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCardSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketContentSlide
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun IdCardDetails(
    card: PocketCardUiModel.IdCard,
    onBack: () -> Unit,
    onShareClick: () -> Unit,
    cardIndex: Int,
) {
    BackHandler { onBack() }

    IdCardDetailsContent(
        card = card,
        onBack = onBack,
        onShareClick = onShareClick,
        cardIndex = cardIndex
    )
}

@Composable
private fun IdCardDetailsContent(
    card: PocketCardUiModel.IdCard,
    onBack: () -> Unit,
    onShareClick: () -> Unit,
    cardIndex: Int
) {
    Column(
        modifier = Modifier.fillMaxSize()
    ) {
        PolkadotTopBar(
            title = stringResource(RCommon.string.identity_card_top_bar_title),
            titleAlignment = TopBarTitleAlignment.Center,
            navigationAction = rememberTopBarAction(onBack),
            actions = persistentListOf(
                rememberTopBarAction(
                    action = onShareClick,
                    icon = NovaIcons.Share
                )
            )
        )

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .windowInsetsPadding(LocalAppNavigationBarInsets.current)
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.mediumIncreased)
            ) {
                IdCard(
                    modifier = Modifier.pocketCardSharedElement(cardIndex),
                    card = card,
                )
            }

            PolkadotSurface(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                    .pocketContentSlide(),
                color = PolkadotTheme.colors.bg.surface.container,
                shape = PolkadotTheme.shapes.extraLarge
            ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    VerticalSpacer { small }

                    NovaText(
                        text = stringResource(RCommon.string.pocket_id_share_subtitle, LocalPaymentAssetBrand.current.symbol)
                            .withCurrencyTickerStyle(PolkadotTheme.typography.paragraph.large),
                        style = PolkadotTheme.typography.paragraph.large,
                        modifier = Modifier.padding(
                            horizontal = PolkadotTheme.spacings.large,
                            vertical = PolkadotTheme.spacings.mediumIncreased
                        )
                    )

                    IdShareQrCard(
                        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                        username = card.username,
                        address = card.address
                    )

                    VerticalSpacer { mediumIncreased }
                }
            }
        }
    }
}

@Preview(apiLevel = 31)
@Composable
private fun IdCardDetailsPreview() = CompositionLocalProvider(
    LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
) {
    PolkadotTheme {
        IdCardDetailsContent(
            card = PocketCardUiModel.IdCard("username.99", "pizza"),
            onBack = {},
            onShareClick = {},
            cardIndex = 0
        )
    }
}
