package io.paritytech.polkadotapp.feature_chats_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.WaitForChatExistsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.toChatFeedPayload
import kotlinx.coroutines.withContext
import javax.inject.Inject

const val CHAT_HOST = "chat"
const val CHAT_LIST_HOST = "chats"
const val CHAT_PARAM_ID = "chatId"

internal class ChatDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val chatsRouter: ChatsRouter,
    private val chatDeeplinkMapper: ChatDeeplinkMapper,
    private val waitForChatExistsUseCase: WaitForChatExistsUseCase,
    private val startChatDataUseCase: StartChatDataUseCase,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri) =
        data.scheme == DeepLinkHandler.APP_SCHEME && data.host == CHAT_HOST

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.io) {
        runCancellableCatching {
            accountRepository.awaitAccountsInitialized()

            val deeplinkPayload = chatDeeplinkMapper.fromDeeplink(data)
                .getOrThrow()

            // The chat with a contact is the chat product's (HX2); a link or an older notification names the account.
            val contact = deeplinkPayload.chatId.contactOrNull()
            val payload = if (contact != null) {
                startChatDataUseCase(contact.contactAccountId).getOrThrow().toChatFeedPayload()
            } else {
                waitForChatExistsUseCase(deeplinkPayload.chatId)
                ChatFeedPayload.existingChat(deeplinkPayload.chatId)
            }

            withContext(coroutineDispatchers.main) {
                chatsRouter.openChatFeed(payload)
            }

            DeeplinkProcessingOutcome.NoOp
        }
    }
}
