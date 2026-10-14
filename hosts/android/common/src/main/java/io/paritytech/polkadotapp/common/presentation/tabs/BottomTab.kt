package io.paritytech.polkadotapp.common.presentation.tabs

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList

enum class BottomTab {
    CHATS,
    WALLET,
    EXPLORE,
    SETTINGS;

    companion object {
        val availableEntries: ImmutableList<BottomTab> = entries
            .filter { FeatureOption.BROWSE_TAB.isEnabled || it != EXPLORE }
            .toImmutableList()
    }
}
