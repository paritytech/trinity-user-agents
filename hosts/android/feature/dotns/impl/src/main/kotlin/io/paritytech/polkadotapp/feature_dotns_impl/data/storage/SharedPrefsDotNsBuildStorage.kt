package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.DotNsNameVersion
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.Json
import timber.log.Timber
import javax.inject.Inject

class SharedPrefsDotNsBuildStorage @Inject constructor(
    @param:ApplicationContext private val context: Context
) : DotNsBuildStorage {
    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    private val builds by lazy { MutableStateFlow(readAll()) }

    override fun get(dotNsName: String): DotNsNameVersion? = builds.value[dotNsName]

    override fun put(dotNsName: String, served: DotNsNameVersion) {
        prefs.edit { putString(dotNsName, Json.encodeToString(served)) }
        builds.update { it + (dotNsName to served) }
    }

    override fun clear() {
        prefs.edit { clear() }
        builds.value = emptyMap()
    }

    private fun readAll(): Map<String, DotNsNameVersion> {
        val storedBuilds = prefs.all.mapValues { (_, value) -> (value as? String)?.let(::decodeOrNull) }
        val unreadableNames = storedBuilds.filterValues { it == null }.keys

        if (unreadableNames.isNotEmpty()) {
            Timber.w("Dropping stored dotNS builds that no longer decode: $unreadableNames")
            prefs.edit { unreadableNames.forEach { remove(it) } }
        }

        return storedBuilds.mapNotNull { (dotNsName, build) -> build?.let { dotNsName to it } }.toMap()
    }

    private fun decodeOrNull(encoded: String): DotNsNameVersion? {
        return runCatching { Json.decodeFromString<DotNsNameVersion>(encoded) }.getOrNull()
    }

    private companion object {
        const val PREFS_NAME = "dotns_names"
    }
}
