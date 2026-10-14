package io.paritytech.polkadotapp.feature_calls_impl.domain.call

import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAvatar
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import javax.inject.Inject

data class CallerDisplay(
    val name: String,
    val avatar: ContactAvatar?,
)

class CallInteractor @Inject constructor(
    private val contactDisplayProvider: ContactDisplayProvider,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
) {
    suspend fun getCallerDisplay(chatId: ChatId): CallerDisplay {
        val accountId = chatId.contactOrNull()?.contactAccountId ?: return CallerDisplay("", null)
        val display = contactDisplayProvider.getContactDisplay(accountId)
        return CallerDisplay(
            name = display?.username ?: fallbackUsernameGenerator.generateFromAccountId(accountId),
            avatar = display?.avatar,
        )
    }
}
