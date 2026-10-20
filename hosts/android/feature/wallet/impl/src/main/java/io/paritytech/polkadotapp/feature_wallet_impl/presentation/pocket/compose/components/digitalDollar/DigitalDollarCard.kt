package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.*
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.common.presentation.compose.TickerWordmark
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
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
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.rememberCountUp
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
    isExpanded: Boolean,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = CardShape,
        color = WalletFallback,
        onClick = { onSelected?.invoke(card) },
        enabled = onSelected != null
    ) {
        CashCardFace(card = card, isExpanded = isExpanded)
    }
}

/**
 * The CASH card's face without any touch handling, so a page can lay it over a list and still let the list
 * scroll from it. [balanceAlpha] fades the balance as the card collapses into a header.
 */
@Composable
fun CashCardFace(
    modifier: Modifier = Modifier,
    card: PocketCardUiModel.DigitalDollar,
    isExpanded: Boolean,
    balanceAlpha: () -> Float = { 1f },
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(CardSizes.HEIGHT)
            .clip(CardShape)
            .background(WalletFallback)
    ) {
        Image(
            modifier = Modifier.matchParentSize(),
            painter = painterResource(R.drawable.img_cash_card_wallet),
            contentDescription = null,
            contentScale = ContentScale.Crop
        )

        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = BalanceStart, bottom = BalanceBottom)
                .graphicsLayer { alpha = balanceAlpha() }
        ) {
            if (isExpanded) {
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
            }

            BalanceAmount(amounts = card.amounts, cardId = card.id)
        }
    }
}

@Composable
private fun BalanceAmount(
    amounts: LoadingState<PocketCardUiModel.DigitalDollar.Amounts>,
    cardId: String,
) {
    val sharedElement = Modifier.pocketBalanceSharedElement(cardId)

    when (amounts) {
        is LoadingState.Loaded -> {
            val balance = rememberCountUp(amounts.data.balance)
            val density = LocalDensity.current
            val shadow = remember(density) {
                with(density) { Shadow(color = BalanceShadow, offset = Offset(0f, 1.dp.toPx()), blurRadius = 2.dp.toPx()) }
            }
            NovaText(
                modifier = sharedElement,
                text = buildAnnotatedString {
                    withStyle(BalanceNumber.toSpanStyle().copy(shadow = shadow)) {
                        append(LocalTokenAmountFormatter.current.formatFiatSigned(balance))
                    }
                    append(" ")
                    withStyle(BalanceUnit.toSpanStyle().copy(shadow = shadow)) {
                        append(LocalPaymentAssetBrand.current.symbol.withCurrencyTickerStyle(BalanceUnit, TickerWordmark.SmallCaps))
                    }
                },
                maxLines = 1,
                style = BalanceNumber,
                color = PolkadotTheme.colors.fg.staticWhite
            )
        }

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

private val CardShape = RoundedCornerShape(23.26.dp)
private val WalletFallback = Color(0xFF3A2417)
private val BalanceShadow = Color.Black.copy(alpha = 0.25f)
private val BalanceStart = 23.dp
private val BalanceBottom = 22.dp
private val BalanceNumber = TextStyle(fontSize = 30.sp, lineHeight = 36.sp, fontWeight = FontWeight(650), letterSpacing = (-0.3).sp)
private val BalanceUnit = TextStyle(fontSize = 24.sp, lineHeight = 28.sp, fontWeight = FontWeight.Light)

private object AmountShimmerSizes {
    val WIDTH = 100.dp
    val HEIGHT = 28.dp
}

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
