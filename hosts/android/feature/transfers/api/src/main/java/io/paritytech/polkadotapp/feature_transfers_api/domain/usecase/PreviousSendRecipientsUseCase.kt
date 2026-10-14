package io.paritytech.polkadotapp.feature_transfers_api.domain.usecase

import io.paritytech.polkadotapp.feature_transfers_api.domain.model.SendRecipientLabeled

interface PreviousSendRecipientsUseCase {
    suspend operator fun invoke(): Result<List<SendRecipientLabeled>>
}
