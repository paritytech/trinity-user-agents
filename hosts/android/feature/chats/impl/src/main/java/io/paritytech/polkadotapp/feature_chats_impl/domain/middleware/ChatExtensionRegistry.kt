package io.paritytech.polkadotapp.feature_chats_impl.domain.middleware

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_impl.domain.extension.CoinagePaymentProcessingExtension
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

typealias ChatExtensionsById = Map<ChatExtensionId, ChatExtension>

@Singleton
class ChatExtensionRegistry @Inject constructor(
    private val staticExtensions: Set<@JvmSuppressWildcards ChatExtension>,
    private val paymentProcessingExtension: CoinagePaymentProcessingExtension,
    private val externalExtensionProvider: ExternalExtensionProvider,
    private val botStateController: ChatBotStateController,
) : CoroutineScope by CoroutineScope(SupervisorJob() + Dispatchers.Default) {
    private val extensionsEnabled = FeatureOption.ALL_CHAT_EXTENSIONS.isEnabled

    private val enabledStaticExtensions = if (extensionsEnabled) staticExtensions else setOf(paymentProcessingExtension)

    private val externalExtensions: SharedFlow<List<ExternalExtension>> = if (extensionsEnabled) {
        externalExtensionProvider.observeExtensions()
    } else {
        flowOf(emptyList())
    }.shareInBackground()

    fun observeExternalExtensions(): Flow<List<ExternalExtension>> {
        return externalExtensions
    }

    suspend fun getAllExtensions(): List<ChatExtension> {
        return enabledStaticExtensions.toList() + externalExtensions.first()
    }

    fun observeActiveExtensions(): Flow<List<ChatExtension>> {
        return combine(
            botStateController.subscribeActiveBotIds(),
            observeAllExtensions()
        ) { activeBotIds, chatExtensions ->
            chatExtensions.filter { it.isActive(activeBotIds) }
        }
    }

    fun observeAllExtensions(): Flow<List<ChatExtension>> {
        val staticExtensionsList = enabledStaticExtensions.toList()
        return externalExtensions.map { externalValues ->
            staticExtensionsList + externalValues
        }
    }

    fun getStaticExtensions(): List<ChatExtension> {
        return enabledStaticExtensions.toList()
    }

    suspend fun getExtension(extensionId: ChatExtensionId): ChatExtension? {
        return getAllExtensions().find { it.id == extensionId }
    }

    suspend fun getExtensionForChat(chatId: ChatId): ChatExtension? = when (val variant = chatId.chatVariant()) {
        is ChatVariant.Extension -> getExtension(variant.extensionId)
        is ChatVariant.Contact -> null
    }

    private fun ChatExtension.isActive(activeExtensionIds: Set<ChatExtensionId>): Boolean {
        return !activationStateExternallyControlled || id in activeExtensionIds
    }
}

fun ChatExtensionRegistry.observeAllExtensionsById(): Flow<ChatExtensionsById> {
    return observeAllExtensions().map { extensions -> extensions.associateBy { it.id } }
}

suspend fun ChatExtensionRegistry.getExtensionForChatResult(chatId: ChatId): Result<ChatExtension> {
    val result = getExtensionForChat(chatId)
    return if (result != null) {
        Result.success(result)
    } else {
        Result.failure(IllegalArgumentException("Chat extension not found for chat $chatId"))
    }
}
