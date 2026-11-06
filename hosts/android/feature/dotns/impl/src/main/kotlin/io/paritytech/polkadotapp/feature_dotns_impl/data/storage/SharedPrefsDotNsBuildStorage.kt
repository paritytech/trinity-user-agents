package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.Json
import javax.inject.Inject

class SharedPrefsDotNsBuildStorage @Inject constructor(
    @param:ApplicationContext private val context: Context
) : DotNsBuildStorage {
    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    private val builds by lazy { MutableStateFlow(readAll()) }

    override fun get(dotNsName: String): DotNsBuild? = builds.value[dotNsName]

    override fun putAll(builds: Map<String, DotNsBuild>) {
        prefs.edit {
            builds.forEach { (dotNsName, build) -> putString(dotNsName, Json.encodeToString(build)) }
        }
        this.builds.update { it + builds }
    }

    override fun all(): Map<String, DotNsBuild> = builds.value

    override fun observeAll(): Flow<Map<String, DotNsBuild>> = builds

    override fun clear() {
        prefs.edit { clear() }
        builds.value = emptyMap()
    }

    private fun readAll(): Map<String, DotNsBuild> {
        return prefs.all.mapNotNull { (dotNsName, value) ->
            (value as? String)?.let { dotNsName to Json.decodeFromString<DotNsBuild>(it) }
        }.toMap()
    }

    private companion object {
        const val PREFS_NAME = "dotns_names"
    }
}
