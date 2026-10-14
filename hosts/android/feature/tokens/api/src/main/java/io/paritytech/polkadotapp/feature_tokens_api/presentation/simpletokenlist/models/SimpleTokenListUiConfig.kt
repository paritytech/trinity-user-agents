package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models

import androidx.annotation.StringRes
import kotlinx.collections.immutable.ImmutableList

class SimpleTokenListUiConfig(
    @param:StringRes val titleRes: Int,
    val titleArgs: ImmutableList<Any>,
)
