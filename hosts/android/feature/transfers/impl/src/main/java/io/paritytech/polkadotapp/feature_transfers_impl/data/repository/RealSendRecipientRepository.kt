package io.paritytech.polkadotapp.feature_transfers_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.database.dao.SendRecipientDao
import io.paritytech.polkadotapp.database.model.SendRecipientLocal
import io.paritytech.polkadotapp.feature_transfers_api.data.repository.SendRecipientRepository
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.SendRecipient
import javax.inject.Inject

class RealSendRecipientRepository @Inject constructor(
    private val dao: SendRecipientDao,
) : SendRecipientRepository {
    override suspend fun getSendRecipients(): Result<List<SendRecipient>> {
        return runCatching { dao.getSendRecipients().map { it.toDomain() } }
    }

    override suspend fun addSendRecipient(recipient: SendRecipient): Result<Unit> {
        return runCatching { dao.insertSendRecipient(recipient.toLocal()) }
    }

    private fun SendRecipient.toLocal(): SendRecipientLocal {
        return SendRecipientLocal(
            accountId.value,
            fullChainAssetId.chainId,
            fullChainAssetId.assetId,
            createdAt
        )
    }

    private fun SendRecipientLocal.toDomain(): SendRecipient {
        return SendRecipient(
            accountId.intoAccountId(),
            FullChainAssetId(chainId, chainAssetId),
            createdAt
        )
    }
}
