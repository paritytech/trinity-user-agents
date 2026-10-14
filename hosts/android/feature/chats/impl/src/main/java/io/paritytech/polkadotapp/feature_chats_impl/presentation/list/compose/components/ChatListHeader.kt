package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Add
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleSize
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.ChatTestTags
import kotlinx.collections.immutable.persistentListOf

@Composable
fun ChatListHeader(
    onAddContactClick: () -> Unit,
    isLoading: Boolean
) {
    Box {
        PolkadotTopBar(
            modifier = Modifier.testTag(ChatTestTags.CHATS_TITLE),
            title = stringResource(R.string.chats_toolbar_title),
            actions = persistentListOf(
                rememberTopBarAction(
                    action = onAddContactClick,
                    icon = NovaIcons.Add,
                ),
            ),
            titleSize = TopBarTitleSize.Large,
        )

        AnimatedVisibility(
            modifier = Modifier.align(Alignment.Center),
            visible = isLoading,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            NovaCircularProgressIndicator(
                modifier = Modifier.size(24.dp),
                strokeWidth = 2.dp
            )
        }
    }
}
