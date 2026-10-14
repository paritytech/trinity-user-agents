package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ReplyPreview.Content.Payment.paymentSubtitle(style: TextStyle): AnnotatedString {
    val formatter = LocalTokenAmountFormatter.current
    val labelRes = when (direction) {
        ChatMessageUiModel.Direction.OUTGOING -> RCommon.string.chat_reply_transfer_outgoing
        ChatMessageUiModel.Direction.INCOMING -> RCommon.string.chat_reply_transfer_incoming
    }
    return stringResource(labelRes, formatter.formatTokenAmount(amount, RoundPrecision.DEFAULT, withSymbol = true))
        .withCurrencyTickerStyle(style)
}
