package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class SharedPrefsContentHashOverrides @Inject constructor(
    @param:ApplicationContext private val context: Context
) : ContentHashOverrides {
    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    override fun getContentHashOverride(dotNsName: String): ContentHash? {
        return prefs.getString(dotNsName, null)
    }

    override fun putContentHashOverride(dotNsName: String, contentHash: ContentHash) {
        prefs.edit { putString(dotNsName, contentHash) }
    }

    companion object {
        private const val PREFS_NAME = "dotns_content_hash_cache"
    }
}
