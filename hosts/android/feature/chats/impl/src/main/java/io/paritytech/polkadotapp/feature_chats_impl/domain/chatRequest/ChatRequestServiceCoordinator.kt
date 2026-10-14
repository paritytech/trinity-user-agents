package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.chatRequest.ChatRequestServiceCoordinator
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealChatRequestServiceCoordinator @Inject constructor(
    private val accountRepository: AccountRepository,
    private val discoveryService: ChatRequestDiscoveryService,
    private val incomingRequestService: IncomingChatRequestService,
    private val coroutineDispatchers: CoroutineDispatchers,
) : ChatRequestServiceCoordinator {
    override suspend fun runChatRequestServices() = withContext(coroutineDispatchers.io) {
        runWalletAccountServices()
        runCandidateAccountServices()
    }

    private suspend fun CoroutineScope.runWalletAccountServices() {
        val walletAccount = accountRepository.getWalletAccount()

        runSharedTopicDiscovery(walletAccount, SharedSecretDerivationDomain.CHAT)
        runSessionTopicDiscovery(walletAccount)
    }

    private suspend fun CoroutineScope.runCandidateAccountServices() {
        val candidateAccount = accountRepository.getCandidateAccount()

        // For candidate account, we only run session topic discovery to require mutual matching before displaying a chat
        runSessionTopicDiscovery(candidateAccount)
    }

    private fun CoroutineScope.runSharedTopicDiscovery(metaAccount: MetaAccount, domain: SharedSecretDerivationDomain) {
        launch {
            runCatching {
                discoveryService.discoverRequests(metaAccount, domain)
            }.logFailure("Failed to discover shared topic requests for ${metaAccount.name}")
        }
    }

    private fun CoroutineScope.runSessionTopicDiscovery(metaAccount: MetaAccount) {
        launch {
            runCatching {
                incomingRequestService.pollForResponses(metaAccount)
            }.logFailure("Failed to discover session topic requests for ${metaAccount.name}")
        }
    }
}
