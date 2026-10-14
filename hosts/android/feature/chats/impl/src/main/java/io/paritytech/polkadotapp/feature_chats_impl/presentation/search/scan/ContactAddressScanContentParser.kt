package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.scan

import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.isValidSubstrateAddress
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getWalletAccountIdIn
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.toChatFeedPayload
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import timber.log.Timber
import javax.inject.Inject

class ContactAddressScanContentParser @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val startChatDataUseCase: StartChatDataUseCase,
    private val router: ChatsRouter
) : ScanContentParser {
    override suspend fun canHandle(content: String): Boolean {
        return content.isValidSubstrateAddress()
    }

    context(scope: ComputationalScope)
    override suspend fun handle(content: String): Result<PostParseAction> {
        return decodeAddress(content)
            .flatMap { accountId -> rejectIfSelf(accountId) }
            .flatMap { accountId -> startChatDataUseCase(accountId).mapError(ContactScanError::ResolveFailed) }
            .map { startChatData -> PostParseAction.BackAndThen { router.openChatFeed(startChatData.toChatFeedPayload()) } }
            .onFailure { Timber.w(it, "Contact address scan rejected: ${it.message}") }
    }

    private fun decodeAddress(content: String): Result<AccountId> {
        return runCatching { content.toAccountId().intoAccountId() }
            .mapError(ContactScanError::NotAnAddress)
    }

    private suspend fun rejectIfSelf(accountId: AccountId): Result<AccountId> = runCatching {
        val selfAccountId = accountRepository.getWalletAccountIdIn(chainRegistry.getChain(knownChains.people))

        if (accountId == selfAccountId) {
            throw ContactScanError.SelfAddress()
        } else accountId
    }
}
