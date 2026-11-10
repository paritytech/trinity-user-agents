package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class SyncContactUsernameUseCase @Inject constructor(
    knownChains: KnownChains,
    private val resourcesRepository: ResourcesRepository,
    private val contactsRepository: ContactsRepository
) {
    private val chainId = knownChains.assetHub

    fun sync(accountId: AccountId): Flow<Result<Unit>> = flowOfAll {
        if (contactsRepository.getContact(accountId)?.username.isFull()) return@flowOfAll emptyFlow()

        resourcesRepository.consumerInfoFlow(chainId, accountId)
            .map { onChainInfo -> writeIfUpgraded(accountId, onChainInfo?.username) }
            .catch { emit(Result.failure(it)) }
    }

    private suspend fun writeIfUpgraded(accountId: AccountId, freshUsername: String?): Result<Unit> = runCatching {
        val storedUsername = (contactsRepository.getContact(accountId) ?: return@runCatching).username

        if (storedUsername.isFull()) return@runCatching

        if (freshUsername != null && freshUsername != storedUsername) {
            contactsRepository.updateContactUsername(accountId, freshUsername)
        }
    }

    private fun String?.isFull(): Boolean = this != null && Username.fromFullValue(this).isFull
}
