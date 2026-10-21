package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.PeopleOutline
import io.paritytech.polkadotapp.common.R as RCommon

/** The button of a product room that opens the list of its participants (02-DESIGN §6.6). */
@Composable
internal fun ProductParticipantsButton(onClick: () -> Unit) {
    val label = stringResource(RCommon.string.chat_participants_title)
    PolkadotIconButton(
        modifier = Modifier.semantics { contentDescription = label },
        icon = NovaIcons.PeopleOutline,
        onClick = onClick,
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotIconButtonSize.mediumIncreased(),
        shape = PolkadotButtonShape.pill,
    )
}
