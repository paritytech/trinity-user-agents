package io.paritytech.polkadotapp.feature_usernames_api.data

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

class LocalFullUsernameStorage(
    delegate: SingleValueStorage<Username>
) : SingleValueStorage<Username> by delegate
