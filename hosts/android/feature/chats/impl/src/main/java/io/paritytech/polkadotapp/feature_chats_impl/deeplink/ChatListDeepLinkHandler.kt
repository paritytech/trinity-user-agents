package io.paritytech.polkadotapp.feature_chats_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class ChatListDeepLinkHandler @Inject constructor(
    private val dispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val chatsRouter: ChatsRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri) =
        data.scheme == DeepLinkHandler.APP_SCHEME && data.host == CHAT_LIST_HOST

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = runCatching {
        accountRepository.awaitAccountsInitialized()

        withContext(dispatchers.main) {
            chatsRouter.openChatsTab()
        }

        DeeplinkProcessingOutcome.NoOp
    }
}
