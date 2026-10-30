package io.paritytech.polkadotapp.common.presentation.theme

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Editor
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import org.junit.Assert.assertEquals
import org.junit.Test

private const val THEME_KEY = "selected_app_theme_id"

class RealAppThemeSelectorTest {
    private val preferences: Preferences = mockk()
    private val editor: Editor = mockk(relaxUnitFun = true)

    @Test
    fun `selected theme starts as the stored theme when a theme id is stored`() {
        withStoredThemeId(PolkadotAppTheme.Tokyo.id)

        assertEquals(PolkadotAppTheme.Tokyo, RealAppThemeSelector(preferences).selectedTheme.value)
    }

    @Test
    fun `selected theme starts as the default when no theme id is stored`() {
        withStoredThemeId(null)

        assertEquals(PolkadotAppTheme.DEFAULT, RealAppThemeSelector(preferences).selectedTheme.value)
    }

    @Test
    fun `selected theme starts as the default when the stored id is unknown`() {
        withStoredThemeId("removed-theme")

        assertEquals(PolkadotAppTheme.DEFAULT, RealAppThemeSelector(preferences).selectedTheme.value)
    }

    @Test
    fun `selected theme and stored id change when a theme is selected`() {
        withStoredThemeId(null)
        withPreferencesEditor()
        val selector = RealAppThemeSelector(preferences)

        selector.select(PolkadotAppTheme.BerlinDay)

        verifyThemeIdStored(PolkadotAppTheme.BerlinDay)
        assertEquals(PolkadotAppTheme.BerlinDay, selector.selectedTheme.value)
    }

    private fun withStoredThemeId(id: String?) {
        every { preferences.getString(THEME_KEY) } returns id
    }

    private fun withPreferencesEditor() {
        every { preferences.edit() } returns editor
    }

    private fun verifyThemeIdStored(theme: PolkadotAppTheme) {
        verify(exactly = 1) { editor.putString(THEME_KEY, theme.id) }
        verify(exactly = 1) { editor.apply() }
    }
}
