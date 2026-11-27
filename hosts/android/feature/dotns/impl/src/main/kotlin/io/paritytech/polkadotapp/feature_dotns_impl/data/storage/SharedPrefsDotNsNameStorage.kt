package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.StoredDotNsName
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.Json
import timber.log.Timber
import javax.inject.Inject

class SharedPrefsDotNsNameStorage @Inject constructor(
    @param:ApplicationContext private val context: Context
) : DotNsNameStorage {
    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    private val storedNames by lazy { MutableStateFlow(readAll()) }

    override fun get(dotNsName: String): StoredDotNsName? = storedNames.value[dotNsName]

    override fun putAll(names: Map<String, StoredDotNsName>) {
        prefs.edit {
            names.forEach { (dotNsName, storedName) -> putString(dotNsName, Json.encodeToString(storedName)) }
        }
        storedNames.update { it + names }
    }

    override fun all(): Map<String, StoredDotNsName> = storedNames.value

    override fun observeAll(): Flow<Map<String, StoredDotNsName>> = storedNames

    override fun clear() {
        prefs.edit { clear() }
        storedNames.value = emptyMap()
    }

    private fun readAll(): Map<String, StoredDotNsName> {
        val decodedNames = prefs.all.mapValues { (_, value) -> (value as? String)?.let(::decodeOrNull) }
        val unreadableNames = decodedNames.filterValues { it == null }.keys

        if (unreadableNames.isNotEmpty()) {
            Timber.w("Dropping stored dotNS names that no longer decode: $unreadableNames")
            prefs.edit { unreadableNames.forEach { remove(it) } }
        }

        return decodedNames.mapNotNull { (dotNsName, storedName) -> storedName?.let { dotNsName to it } }.toMap()
    }

    private fun decodeOrNull(encoded: String): StoredDotNsName? {
        return runCatching { Json.decodeFromString<StoredDotNsName>(encoded) }.getOrNull()
    }

    private companion object {
        const val PREFS_NAME = "dotns_names"
    }
}
