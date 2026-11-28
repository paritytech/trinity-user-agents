package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsOverride
import org.junit.Assert.assertEquals
import org.junit.Test

class PrefsMaterialsDebugSettingsTest {
    private val prefs = MapSharedPreferences()

    @Test
    fun `follows Remote Config until the debug menu says otherwise`() {
        val settings = PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = true)

        assertEquals(MaterialsOverride.REMOTE_CONFIG, settings.override())
    }

    @Test
    fun `persists the override across instances`() {
        PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = true).setOverride(MaterialsOverride.ON)

        assertEquals(MaterialsOverride.ON, PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = true).override())
    }

    @Test
    fun `a build without debug tools always follows Remote Config`() {
        PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = true).setOverride(MaterialsOverride.OFF)

        assertEquals(MaterialsOverride.REMOTE_CONFIG, PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = false).override())
    }

    @Test
    fun `an unknown stored value reads as no override`() {
        prefs.edit().putString("materials_override", "SOMETIMES").apply()

        assertEquals(MaterialsOverride.REMOTE_CONFIG, PrefsMaterialsDebugSettings(prefs, debugToolsEnabled = true).override())
    }
}
