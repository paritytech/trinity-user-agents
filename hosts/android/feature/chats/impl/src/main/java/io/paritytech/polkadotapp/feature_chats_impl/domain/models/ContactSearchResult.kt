package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

class ContactSearchResult(
    val username: Username,
    val accountId: AccountId,
)
