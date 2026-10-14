package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import javax.inject.Inject

class RealDotNsTldStorage @Inject constructor(
    private val preferences: Preferences,
) : DotNsTldStorage {
    override fun getTld(): DotNsTld? {
        return preferences.getString(KEY_TLD)?.let(DotNsTld::parse)
    }

    override fun putTld(tld: DotNsTld) {
        preferences.putString(KEY_TLD, tld.value)
    }

    private companion object {
        const val KEY_TLD = "dotns_tld"
    }
}
