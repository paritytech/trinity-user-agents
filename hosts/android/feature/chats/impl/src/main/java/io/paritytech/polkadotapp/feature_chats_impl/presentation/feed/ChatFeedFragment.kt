package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.fragment.app.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.formatters.space.InformationSizeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.space.LocalInformationSizeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.common.presentation.tabbar.BottomNavHeightProvider
import io.paritytech.polkadotapp.common.utils.rememberCurrentTimeMillisWithDelay
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.LocalMultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.MultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.ChatFeedScreen
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.LocalChatFeedTimestampAnchor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

@AndroidEntryPoint
class ChatFeedFragment : BaseComposeFragment<ChatFeedViewModel>() {
    override val viewModel: ChatFeedViewModel by viewModels()

    @Inject
    lateinit var chatMessageTimeFormatter: ChatMessageTimeFormatter

    @Inject
    lateinit var tokenAmountFormatter: TokenAmountFormatter

    @Inject
    lateinit var informationSizeFormatter: InformationSizeFormatter

    @Inject
    lateinit var timeFormatter: TimeFormatter

    @Inject
    lateinit var multimediaMessageController: MultimediaMessageController

    @Inject
    lateinit var bottomNavHeightProvider: BottomNavHeightProvider

    @Composable
    override fun Screen() {
        val anchorTimestamp by rememberCurrentTimeMillisWithDelay(1.minutes)
        val bottomNavHeight by bottomNavHeightProvider.heightDp.collectAsStateWithLifecycle()

        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides chatMessageTimeFormatter,
            LocalTimeFormatter provides timeFormatter,
            LocalTokenAmountFormatter provides tokenAmountFormatter,
            LocalInformationSizeFormatter provides informationSizeFormatter,
            LocalChatFeedTimestampAnchor provides anchorTimestamp,
            LocalMultimediaMessageController provides multimediaMessageController,
            LocalAppNavigationBarInsets provides WindowInsets(bottom = bottomNavHeight),
        ) {
            ChatFeedScreen(viewModel)
        }
    }
}
