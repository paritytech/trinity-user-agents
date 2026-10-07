package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PrefsProductRuntimeSettingsTest {
    private val prefs = MapSharedPreferences()

    @Test
    fun `defaults to the TrUAPI runtime`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = true)

        assertTrue(settings.isTrUAPIRuntimeEnabled())
    }

    @Test
    fun `persists an opt-out to the native host`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = true)

        settings.setTrUAPIRuntimeEnabled(false)

        assertFalse(settings.isTrUAPIRuntimeEnabled())
        assertFalse(PrefsProductRuntimeSettings(prefs, isDebugBuild = true).isTrUAPIRuntimeEnabled())
    }

    @Test
    fun `release builds run native despite the TrUAPI default`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = false)

        assertFalse(settings.isTrUAPIRuntimeEnabled())

        settings.setTrUAPIRuntimeEnabled(true)

        assertFalse(settings.isTrUAPIRuntimeEnabled())
    }

    // The opt-in moves one product's worker and chat onto the core; any other product, or any
    // release build, has to keep the runtime it had.
    @Test
    fun `a wasmi opt-in covers only its product and never a release build`() {
        val debug = PrefsProductRuntimeSettings(prefs, isDebugBuild = true)

        debug.setWasmiWorkerProduct(COUNTER, enabled = true)

        assertTrue(debug.isWasmiWorkerProduct(COUNTER))
        assertFalse(debug.isWasmiWorkerProduct(OTHER))
        assertFalse(debug.isWasmiWorkerRuntimeEnabled())
        assertFalse(PrefsProductRuntimeSettings(prefs, isDebugBuild = false).isWasmiWorkerProduct(COUNTER))
    }

    private companion object {
        val COUNTER = ProductId.fromStoredValue("counter.paseo")
        val OTHER = ProductId.fromStoredValue("dim2.paseo")
    }
}

private class MapSharedPreferences : SharedPreferences {
    private val values = mutableMapOf<String, Any?>()

    override fun getBoolean(key: String, defValue: Boolean): Boolean =
        values.getOrDefault(key, defValue) as Boolean

    override fun edit(): SharedPreferences.Editor = object : SharedPreferences.Editor {
        private val pending = mutableMapOf<String, Any?>()

        override fun putBoolean(key: String, value: Boolean) = apply { pending[key] = value }
        override fun putString(key: String, value: String?) = apply { pending[key] = value }
        override fun putStringSet(key: String, value: MutableSet<String>?) = apply { pending[key] = value }
        override fun putInt(key: String, value: Int) = apply { pending[key] = value }
        override fun putLong(key: String, value: Long) = apply { pending[key] = value }
        override fun putFloat(key: String, value: Float) = apply { pending[key] = value }
        override fun remove(key: String) = apply { pending[key] = REMOVE }
        override fun clear() = apply { values.clear() }

        override fun commit(): Boolean {
            apply()
            return true
        }

        override fun apply() {
            pending.forEach { (key, value) ->
                if (value === REMOVE) values.remove(key) else values[key] = value
            }
            pending.clear()
        }
    }

    override fun getAll(): MutableMap<String, *> = values
    override fun getString(key: String, defValue: String?): String? = values.getOrDefault(key, defValue) as String?

    @Suppress("UNCHECKED_CAST")
    override fun getStringSet(key: String, defValues: MutableSet<String>?): MutableSet<String>? =
        values.getOrDefault(key, defValues) as MutableSet<String>?

    override fun getInt(key: String, defValue: Int): Int = values.getOrDefault(key, defValue) as Int
    override fun getLong(key: String, defValue: Long): Long = values.getOrDefault(key, defValue) as Long
    override fun getFloat(key: String, defValue: Float): Float = values.getOrDefault(key, defValue) as Float
    override fun contains(key: String): Boolean = values.containsKey(key)

    override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) = Unit
    override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) = Unit

    private companion object {
        val REMOVE = Any()
    }
}
