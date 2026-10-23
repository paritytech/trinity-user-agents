package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject

class SharedPrefsDotNsBuildStorage @Inject constructor(
    @param:ApplicationContext private val context: Context
) : DotNsBuildStorage {
    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    private val builds by lazy { MutableStateFlow(readAll()) }

    override fun get(dotNsName: String): DotNsBuild? = builds.value[dotNsName]

    override fun put(dotNsName: String, build: DotNsBuild) {
        prefs.edit {
            putString(servedKey(dotNsName), build.served)
            if (build.update != null) putString(updateKey(dotNsName), build.update) else remove(updateKey(dotNsName))
        }
        builds.update { it + (dotNsName to build) }
    }

    override fun all(): Map<String, DotNsBuild> = builds.value

    override fun observeAll(): Flow<Map<String, DotNsBuild>> = builds

    override fun clear() {
        prefs.edit { clear() }
        builds.value = emptyMap()
    }

    private fun readAll(): Map<String, DotNsBuild> {
        val entries = prefs.all.mapNotNull { (key, value) -> (value as? String)?.let { key to it } }.toMap()

        return entries.keys
            .filter { it.startsWith(SERVED_PREFIX) }
            .associate { key ->
                val dotNsName = key.removePrefix(SERVED_PREFIX)
                dotNsName to DotNsBuild(served = entries.getValue(key), update = entries[updateKey(dotNsName)])
            }
    }

    private fun servedKey(dotNsName: String) = SERVED_PREFIX + dotNsName

    private fun updateKey(dotNsName: String) = UPDATE_PREFIX + dotNsName

    private companion object {
        const val PREFS_NAME = "dotns_builds"
        const val SERVED_PREFIX = "served:"
        const val UPDATE_PREFIX = "update:"
    }
}
