package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CardReceived
import io.paritytech.polkadotapp.design.components.icon.vectors.CardSent
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.funding.reasonText
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.text
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCard
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingInfoRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingTimelineCard
import kotlinx.collections.immutable.ImmutableList
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlin.time.Instant
import kotlin.time.toJavaInstant
import io.paritytech.polkadotapp.common.R as RCommon

private val ICON_SIZE = 40.dp
private val GLYPH_SIZE = 20.dp
private val COPY_ICON_SIZE = 16.dp
private const val TINT_ALPHA = 0.24f

@Composable
fun FundingEndedScreen(viewModel: FundingEndedViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    when (val loaded = state) {
        is LoadingState.Loaded -> FundingEndedContent(
            state = loaded.data,
            onClose = viewModel::onClose,
            onStartOver = viewModel::onStartOver,
            onCopy = viewModel::onCopy,
        )
        else -> Box(modifier = Modifier.fillMaxWidth())
    }
}

@Composable
private fun FundingEndedContent(
    state: FundingEndedScreenState,
    onClose: () -> Unit,
    onStartOver: () -> Unit,
    onCopy: (String) -> Unit,
) {
    val ended = state.ended

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(title = "", onBack = onClose)

        Column(
            modifier = Modifier
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
        ) {
            Headline(ended)
            FundingTimelineCard(timeline = ended.timeline, band = ended.note?.let { text(it) }, bandWarning = false)
            Rows(rows = ended.rows, providerName = state.providerName, onCopy = onCopy)
        }

        when (ended.action) {
            FundingEndedAction.CLOSE -> PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.funding_failure_close),
                style = PolkadotButtonStyle.secondary(),
                onClick = onClose,
            )
            FundingEndedAction.START_OVER -> PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.funding_ended_start_over),
                onClick = onStartOver,
            )
        }
    }
}

@Composable
private fun Headline(ended: FundingEndedUiState) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        StatusIcon(ended.icon)
        if (ended.figure != null) {
            NovaText(
                text = "${ended.figure} ${LocalPaymentAssetBrand.current.symbol}",
                style = PolkadotTheme.typography.headline.large,
                color = when (ended.figureTone) {
                    FundingEndedTone.SUCCESS -> PolkadotTheme.colors.fg.success
                    FundingEndedTone.PRIMARY -> PolkadotTheme.colors.fg.primary
                    FundingEndedTone.TERTIARY -> PolkadotTheme.colors.fg.tertiary
                },
                maxLines = 1,
            )
        }
        if (ended.settledAt != null) {
            NovaText(
                text = dateTime(ended.settledAt),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }
    }
}

@Composable
private fun StatusIcon(icon: FundingEndedIcon) {
    val (background, tint) = when (icon) {
        FundingEndedIcon.IN -> PolkadotTheme.colors.bg.action.primary to PolkadotTheme.colors.fg.primaryInverted
        FundingEndedIcon.OUT -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingEndedIcon.FAILED -> PolkadotTheme.colors.fg.error.copy(alpha = TINT_ALPHA) to PolkadotTheme.colors.fg.error
    }
    PolkadotSurface(
        modifier = Modifier.size(ICON_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = background,
        contentAlignment = Alignment.Center,
    ) {
        NovaIcon(
            modifier = Modifier.size(GLYPH_SIZE),
            imageVector = when (icon) {
                FundingEndedIcon.IN -> NovaIcons.CardReceived
                FundingEndedIcon.OUT -> NovaIcons.CardSent
                FundingEndedIcon.FAILED -> NovaIcons.Close
            },
            tint = tint,
        )
    }
}

@Composable
private fun Rows(rows: ImmutableList<FundingEndedRow>, providerName: String?, onCopy: (String) -> Unit) {
    FundingCard {
        rows.forEach { row ->
            when (row) {
                is FundingEndedRow.Settled -> FundingInfoRow(label = stringResource(row.caption)) {
                    Column(horizontalAlignment = Alignment.End) {
                        Value(row.value)
                        if (row.requested != null) {
                            NovaText(
                                text = stringResource(RCommon.string.funding_ended_requested, row.requested),
                                style = PolkadotTheme.typography.body.small,
                                color = PolkadotTheme.colors.fg.secondary,
                            )
                        }
                    }
                }
                is FundingEndedRow.Value -> FundingInfoRow(label = stringResource(row.caption)) { Value(text(row.value)) }
                is FundingEndedRow.Provider -> if (providerName != null) {
                    FundingInfoRow(label = stringResource(RCommon.string.funding_summary_provider)) { Value(providerName) }
                }
                is FundingEndedRow.Copy -> FundingInfoRow(
                    modifier = Modifier.clickable { onCopy(row.value) },
                    label = stringResource(row.caption),
                ) {
                    NovaText(
                        modifier = Modifier.weight(1f, fill = false),
                        text = row.value,
                        style = PolkadotTheme.typography.body.mediumEmphasized,
                        color = PolkadotTheme.colors.fg.primary,
                        maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                    )
                    NovaIcon(
                        modifier = Modifier.size(COPY_ICON_SIZE),
                        imageVector = NovaIcons.ContentCopy,
                        tint = PolkadotTheme.colors.fg.secondary,
                    )
                }
                is FundingEndedRow.Moment -> FundingInfoRow(label = stringResource(row.caption)) { Value(dateTime(row.at)) }
            }
        }
    }
}

@Composable
private fun Value(text: String) {
    NovaText(
        text = text,
        style = PolkadotTheme.typography.body.mediumEmphasized,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.End,
    )
}

@Composable
private fun text(text: FundingEndedText): String = when (text) {
    is FundingEndedText.Res -> stringResource(text.res, *text.args.toTypedArray())
    is FundingEndedText.Reason -> text.reason.reasonText.text()
}

private val DATE_TIME = DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)

private fun dateTime(at: Instant): String = at.toJavaInstant().atZone(ZoneId.systemDefault()).format(DATE_TIME)
