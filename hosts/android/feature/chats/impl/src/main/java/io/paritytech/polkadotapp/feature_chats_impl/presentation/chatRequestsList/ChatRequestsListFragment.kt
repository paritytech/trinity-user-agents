package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose.ChatRequestsListScreen
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import javax.inject.Inject

@AndroidEntryPoint
class ChatRequestsListFragment : BaseComposeFragment<ChatRequestsListViewModel>() {
    override val viewModel: ChatRequestsListViewModel by viewModels()

    @Inject
    lateinit var chatMessageTimeFormatter: ChatMessageTimeFormatter

    @Composable
    override fun Screen() {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides chatMessageTimeFormatter
        ) {
            ChatRequestsListScreen(contract = viewModel)
        }
    }
}
