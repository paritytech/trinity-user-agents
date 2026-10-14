package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.ChatSearchScreen
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import javax.inject.Inject

@AndroidEntryPoint
internal class ChatSearchFragment : BaseComposeFragment<ChatSearchViewModel>() {
    override val viewModel: ChatSearchViewModel by viewModels()

    @Inject
    lateinit var chatMessageTimeFormatter: ChatMessageTimeFormatter

    @Composable
    override fun Screen() {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides chatMessageTimeFormatter
        ) {
            ChatSearchScreen()
        }
    }
}
