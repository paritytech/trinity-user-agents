package io.paritytech.polkadotapp.feature_usernames_api.data

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

typealias LocalUsernameStorage = SingleValueStorage<Username>
