package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.*
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetLogoVariant
import io.paritytech.polkadotapp.common.presentation.paymentAsset.compose.PaymentAssetLogoImage
import io.paritytech.polkadotapp.common.presentation.paymentAsset.compose.aspectRatio
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Refreshing
import io.paritytech.polkadotapp.design.components.icon.vectors.WarningFilled
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.LocalCardTilt
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.MotionShineParameters
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.maskedMotionShine
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.CardSizes
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketBalanceSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarBalanceStatus
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun DigitalDollarCard(
    modifier: Modifier = Modifier,
    card: PocketCardUiModel.DigitalDollar,
    onSelected: ((PocketCardUiModel.DigitalDollar) -> Unit)? = null,
    isExpanded: Boolean
) {
    val tiltState = LocalCardTilt.current

    val highlightBrush = remember { digitalDollarHighlightBrush() }

    val litBorderColor = PolkadotTheme.colors.fg.staticWhite
    val borderBrush = remember(litBorderColor) { digitalDollarBorderBrush(litBorderColor) }

    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PocketCardColors.DigitalDollarCardBackground,
        border = BorderStroke(BORDER_WIDTH, borderBrush),
        onClick = { onSelected?.invoke(card) },
        enabled = onSelected != null
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(CardSizes.HEIGHT)
                .background(highlightBrush)
        )

        Image(
            modifier = Modifier.matchParentSize(),
            painter = painterResource(R.drawable.img_texture_grain_dark),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            alpha = TEXTURE_ALPHA
        )

        Box(modifier = Modifier.matchParentSize()) {
            Image(
                modifier = Modifier
                    .fillMaxHeight()
                    .align(Alignment.CenterEnd)
                    .maskedMotionShine(
                        tiltState = tiltState,
                        parameters = MotionShineParameters.DigitalDollarCard,
                        contentAlpha = ILLUSTRATION_ALPHA
                    ),
                painter = painterResource(R.drawable.img_digital_dollar_card),
                contentDescription = null,
                contentScale = ContentScale.FillHeight
            )
        }

        Box(
            modifier = Modifier
                .matchParentSize()
                .padding(
                    horizontal = PolkadotTheme.spacings.large,
                    vertical = PolkadotTheme.spacings.mediumIncreased
                )
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Row(
                    modifier = Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    PaymentAssetLogoImage(
                        modifier = Modifier
                            .height(LogoHeight)
                            .aspectRatio(PaymentAssetLogoVariant.Square.aspectRatio),
                        variant = PaymentAssetLogoVariant.Square
                    )
                }

                BalanceAmount(
                    amounts = card.amounts,
                    cardId = card.id,
                    isHidden = isExpanded
                )
            }

            if (isExpanded) {
                Column(
                    modifier = Modifier.align(Alignment.BottomStart)
                ) {
                    AnimatedContent(
                        targetState = card.balanceStatus,
                        contentKey = { it::class },
                        label = "DigitalDollarBalanceStatus"
                    ) { status ->
                        when (status) {
                            DigitalDollarBalanceStatus.Syncing -> SyncProgress()
                            DigitalDollarBalanceStatus.AccountBackupPending -> AccountBackupPending()
                            is DigitalDollarBalanceStatus.PartlyReady -> PartlyReadyBalance(amount = status.amount)

                            DigitalDollarBalanceStatus.TotalOnly -> Unit
                        }
                    }

                    BalanceAmount(
                        amounts = card.amounts,
                        cardId = card.id,
                        isHidden = false
                    )
                }
            }
        }
    }
}

@Composable
private fun BalanceAmount(
    amounts: LoadingState<PocketCardUiModel.DigitalDollar.Amounts>,
    cardId: String,
    isHidden: Boolean
) {
    val sharedElement = if (isHidden) Modifier.alpha(0f) else Modifier.pocketBalanceSharedElement(cardId)

    when (amounts) {
        is LoadingState.Loaded -> NovaText(
            modifier = sharedElement,
            text = LocalTokenAmountFormatter.current.formatFiatSigned(amounts.data.balance, withSymbol = true)
                .withCurrencyTickerStyle(PolkadotTheme.typography.headline.medium),
            maxLines = 1,
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.staticWhite
        )

        else -> Shimmer(
            modifier = sharedElement.size(width = AmountShimmerSizes.WIDTH, height = AmountShimmerSizes.HEIGHT),
            shape = PolkadotTheme.shapes.small
        )
    }
}

@Composable
private fun PartlyReadyBalance(amount: TokenAmountModel) {
    Column {
        NovaText(
            text = stringResource(RCommon.string.pocket_coinage_ready),
            maxLines = 1,
            style = PolkadotTheme.typography.body.medium,
            color = PocketCardColors.Secondary
        )

        NovaText(
            text = LocalTokenAmountFormatter.current.formatFiatSigned(amount, withSymbol = true)
                .withCurrencyTickerStyle(PolkadotTheme.typography.body.medium),
            maxLines = 1,
            style = PolkadotTheme.typography.body.medium,
            color = PocketCardColors.Primary
        )

        VerticalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.pocket_coinage_total_balance),
            maxLines = 1,
            style = PolkadotTheme.typography.body.medium,
            color = PocketCardColors.Secondary
        )
    }
}

@Composable
private fun SyncProgress() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        val infiniteTransition = rememberInfiniteTransition()
        val angle by infiniteTransition.animateFloat(
            initialValue = 0F,
            targetValue = -360F,
            animationSpec = infiniteRepeatable(
                animation = tween(2000, easing = LinearEasing)
            )
        )
        NovaIcon(
            modifier = Modifier
                .size(18.dp)
                .rotate(angle),
            imageVector = NovaIcons.Refreshing,
            tint = PocketCardColors.Primary
        )

        HorizontalSpacer { tiny }

        NovaText(
            text = stringResource(RCommon.string.asset_details_backup_in_progress),
            style = PolkadotTheme.typography.body.medium,
            color = PocketCardColors.Primary
        )
    }
}

@Composable
private fun AccountBackupPending() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        NovaIcon(
            modifier = Modifier.size(18.dp),
            imageVector = NovaIcons.WarningFilled,
            tint = PocketCardColors.Primary
        )

        HorizontalSpacer { tiny }

        NovaText(
            text = stringResource(RCommon.string.pocket_digital_dollar_account_backup_pending),
            style = PolkadotTheme.typography.body.medium,
            color = PocketCardColors.Primary
        )
    }
}

private val BORDER_WIDTH = 0.5.dp

private const val ILLUSTRATION_ALPHA = 0.2f

private const val TEXTURE_ALPHA = 0.3f

private object AmountShimmerSizes {
    val WIDTH = 100.dp
    val HEIGHT = 28.dp
}

private val LogoHeight = 36.dp

@Preview
@Composable
private fun DigitalDollarCardPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loaded(
            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
        ),
        syncInProgress = true,
        isExpanded = true
    )
}

@Preview
@Composable
private fun DigitalDollarCardPartlyReadyPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loaded(
            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock(30), TokenAmountModel.mock(20))
        ),
        syncInProgress = false,
        isExpanded = true
    )
}

@Preview
@Composable
private fun DigitalDollarCardAccountBackupPendingPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loaded(
            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
        ),
        syncInProgress = false,
        isExpanded = true,
        accountBackupPending = true,
    )
}

@Preview
@Composable
private fun DigitalDollarCardCollapsedPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loaded(
            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
        ),
        syncInProgress = false,
        isExpanded = false
    )
}

@Preview
@Composable
private fun DigitalDollarCardLoadingPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loading,
        syncInProgress = false,
        isExpanded = true
    )
}

@Preview
@Composable
private fun DigitalDollarCardCollapsedLoadingPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loading,
        syncInProgress = false,
        isExpanded = false
    )
}

@Preview
@Composable
private fun DigitalDollarCardSyncingWhileLoadingPreview() {
    DigitalDollarCardPreviewContainer(
        amounts = LoadingState.Loading,
        syncInProgress = true,
        isExpanded = true
    )
}

@Composable
private fun DigitalDollarCardPreviewContainer(
    amounts: LoadingState<PocketCardUiModel.DigitalDollar.Amounts>,
    syncInProgress: Boolean,
    isExpanded: Boolean,
    accountBackupPending: Boolean = false,
) {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
        ) {
            DigitalDollarCard(
                card = PocketCardUiModel.DigitalDollar(
                    amounts = amounts,
                    syncInProgress = syncInProgress,
                    accountBackupPending = accountBackupPending,
                ),
                isExpanded = isExpanded
            )
        }
    }
}
