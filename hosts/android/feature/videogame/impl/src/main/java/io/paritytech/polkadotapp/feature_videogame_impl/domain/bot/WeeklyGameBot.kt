@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_videogame_impl.domain.bot

import android.content.Context
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.getResourceUri
import io.paritytech.polkadotapp.designsystem.colors.BerlinNightPalette
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBot
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOverlayRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.createAttachmentFromDrawable
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import io.paritytech.polkadotapp.feature_upgrade_username_api.domain.bot.UsernameUpgradedMessageProcessor
import io.paritytech.polkadotapp.feature_upgrade_username_api.presentation.bot.UsernameUpgradedRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.R
import io.paritytech.polkadotapp.feature_videogame_impl.domain.bot.depositProcessing.DepositMessageProcessor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.bot.messages.deposit.DepositAddedRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.domain.bot.statusProcessing.GameStatusMessageProcessor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.VideoGameChatBotInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.background.WeeklyGameBackgroundRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.footer.WeeklyGameBotFooterRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.menu.WeeklyGameSettingsMenuRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.GamePillOverlayRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.WeeklyGamePillOverlayViewModel
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.renderer.GameResultRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.theme.WeeklyGamePrizesBubble
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.theme.WeeklyGamePrizesDateSeparator
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.play.VideoGamePlayFragment
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.voting.VideoGameVotingFragment
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

@Singleton
internal class WeeklyGameBot @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val interactor: VideoGameChatBotInteractor,
    private val gameResultRenderer: GameResultRenderer,
    private val depositAddedRenderer: DepositAddedRenderer,
    private val usernameUpgradedRenderer: UsernameUpgradedRenderer,
    private val gameStatusMessageProcessor: GameStatusMessageProcessor,
    private val depositMessageProcessor: DepositMessageProcessor,
    override val customChatPreviewDelegate: WeeklyGameChatPreviewDelegate,
    override val customOpenHandler: WeeklyGameOpenHandler,
    private val usernameUpgradedMessageProcessor: UsernameUpgradedMessageProcessor,
) : ChatBot() {
    private val data = ChatBotData.weeklyGame()

    override val id: ChatExtensionId = data.id
    override val metadata = ChatBotMetadata(
        name = data.name,
        icon = context.getResourceUri(R.drawable.chat_bot_prizes_icon),
    )

    override val customFooterRenderer: CustomChatFooterRenderer = WeeklyGameBotFooterRenderer()

    override val customMenuRenderer: CustomChatMenuRenderer = WeeklyGameSettingsMenuRenderer()

    override val customGlobalOverlayRenderer: CustomChatOverlayRenderer =
        GamePillOverlayRenderer { hiltViewModel<WeeklyGamePillOverlayViewModel>() }

    override val customChatAppearance: CustomChatAppearance = CustomChatAppearance(
        backgroundRenderer = WeeklyGameBackgroundRenderer(),
        bubbleStyle = WeeklyGamePrizesBubble,
        dateSeparatorStyle = WeeklyGamePrizesDateSeparator,
        toolbarPalette = BerlinNightPalette(),
    )

    override val animatesMessageReveal: Boolean = true

    override fun ownedFragmentClasses(): Set<String> = setOf(
        VideoGamePlayFragment::class.java.name,
        VideoGameVotingFragment::class.java.name,
    )

    override fun customMessageRenderers(): List<CustomChatMessageRenderer<*>> {
        return listOf(gameResultRenderer, usernameUpgradedRenderer, depositAddedRenderer)
    }

    context(chatBotContext: ChatBotContext)
    override fun startBotWork() {
        interactor.startUpdates().launchIn(chatBotContext.scope)

        chatBotContext.scope.launch {
            chatBotContext.setWelcomeMessages { createWelcomeMessages() }

            launchGameProgressObserver()

            gameStatusMessageProcessor.launchSendingMessages()
            usernameUpgradedMessageProcessor.launchSendingMessages()
            depositMessageProcessor.launchSendingMessages()
        }
    }

    context(chatBotContext: ChatBotContext)
    private fun launchGameProgressObserver() {
        chatBotContext.scope.launch { interactor.observeGameProgressAndClaimCitizenship() }
        chatBotContext.scope.launch { interactor.observeGameProgressAndStartPersonSetup() }
    }

    override fun observeUserInputAllowed(): Flow<Boolean> = flowOf(false)

    override fun observeChatConfig() = flowOf(
        ChatConfig(
            showTimestamps = false,
            showAvatar = false,
            showNewMessagesSeparator = false,
        )
    )

    private fun createWelcomeMessages(): List<ChatMessage.Content> = listOf(
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_1),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_1))
        ),
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_2),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_2))
        ),
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_3),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_3))
        ),
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_4),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_4))
        ),
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_5),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_5))
        ),
        ChatMessage.Content.RichText(
            text = context.getString(RCommon.string.chat_bot_prizes_intro_message_6),
            attachments = listOf(context.createAttachmentFromDrawable(R.drawable.chat_bot_prizes_intro_6))
        ),
    )
}
