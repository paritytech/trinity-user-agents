package io.paritytech.polkadotapp.feature_transfers_impl.domain

import io.paritytech.polkadotapp.common.data.memory.AccumulatingMapCache
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.feature_transfers_api.data.repository.SendRecipientRepository
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.SendRecipient
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.SendRecipientLabeled
import io.paritytech.polkadotapp.feature_transfers_api.domain.usecase.PreviousSendRecipientsUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ResolveUsernamesUseCase
import javax.inject.Inject

class RealPreviousSendRecipientsUseCase @Inject constructor(
    private val resolveUsernameUseCase: ResolveUsernamesUseCase,
    private val sendRecipientRepository: SendRecipientRepository,
) : PreviousSendRecipientsUseCase {
    private val resolvedUsernamesCache = AccumulatingMapCache {
        resolveUsernameUseCase(it)
    }

    override suspend operator fun invoke(): Result<List<SendRecipientLabeled>> {
        return sendRecipientRepository.getSendRecipients().mapCatching { it.mapToLabeled() }
    }

    private suspend fun List<SendRecipient>.mapToLabeled(): List<SendRecipientLabeled> {
        val usernames = resolvedUsernamesCache.get(this.map { it.accountId }).getOrEmpty()

        return this.map {
            SendRecipientLabeled(
                label = usernames[it.accountId]?.getDisplayUsername(),
                fullChainAssetId = it.fullChainAssetId,
                createdAt = it.createdAt,
                accountId = it.accountId
            )
        }
    }
}
