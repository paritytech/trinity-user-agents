package io.paritytech.polkadotapp.feature_chats_impl.utils

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import javax.inject.Inject

class ChatMessageMappingHelper @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val dollarAssetProvider: ChainAssetProvider,
    private val tokenAmountMapper: TokenAmountMapper,
) {
    suspend fun extractTokenAmount(paymentContent: ChatMessage.Content.CoinagePayment): TokenAmountModel {
        return extractTokenAmount(paymentContent.totalValue)
    }

    suspend fun extractTokenAmount(totalValue: Balance): TokenAmountModel {
        return dollarAssetProvider.extractAmount(totalValue)
    }

    suspend fun mapPaymentStatus(
        status: ChatMessage.Content.CoinagePayment.Status
    ): ChatMessageUiModel.CoinagePayment.Status {
        return when (status) {
            is ChatMessage.Content.CoinagePayment.Status.Detecting -> ChatMessageUiModel.CoinagePayment.Status.Detecting
            is ChatMessage.Content.CoinagePayment.Status.Detected ->
                ChatMessageUiModel.CoinagePayment.Status.Detected(dollarAssetProvider.extractAmount(status.amount))

            is ChatMessage.Content.CoinagePayment.Status.PartiallyClaimed ->
                ChatMessageUiModel.CoinagePayment.Status.PartiallyClaimed(dollarAssetProvider.extractAmount(status.claimed))

            is ChatMessage.Content.CoinagePayment.Status.Transferred ->
                ChatMessageUiModel.CoinagePayment.Status.Transferred(dollarAssetProvider.extractAmount(status.amount))
            is ChatMessage.Content.CoinagePayment.Status.FailedDetection -> ChatMessageUiModel.CoinagePayment.Status.FailedDetection
            is ChatMessage.Content.CoinagePayment.Status.FailedTransfer -> ChatMessageUiModel.CoinagePayment.Status.FailedTransfer
        }
    }

    private suspend fun ChainAssetProvider.extractAmount(balance: Balance): TokenAmountModel {
        val asset = this.asset()
        val assetWithAmount = asset.withAmount(balance)

        return tokenAmountMapper.mapFrom(assetWithAmount)
    }
}
