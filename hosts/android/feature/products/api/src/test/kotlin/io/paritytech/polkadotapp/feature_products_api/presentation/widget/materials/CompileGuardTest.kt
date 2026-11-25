package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.content.SharedPreferences
import io.mockk.every
import io.mockk.mockk
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CompileGuardTest {
    private val material = ByteArray(32) { 0xab.toByte() }
    private val key = "ab".repeat(32)
    private val editor = mockk<SharedPreferences.Editor>(relaxed = true) {
        every { putBoolean(any(), any()) } returns this
        every { remove(any()) } returns this
        every { commit() } returns true
    }

    private fun prefsHolding(vararg keys: String) = mockk<SharedPreferences> {
        every { edit() } returns editor
        every { all } returns keys.associateWith { true }
    }

    @Test
    fun `a marker left by an earlier process means that material crashed`() {
        val guard = CompileGuard(prefsHolding(key))

        assertTrue(guard.crashed(material))
        assertFalse(guard.crashed(ByteArray(32)))
    }

    // Compiles run on the GL thread while the main thread asks; one in progress is not a crash.
    @Test
    fun `a compile in progress in this process is not a crash`() {
        val prefs = prefsHolding()
        val guard = CompileGuard(prefs)
        guard.crashed(material)

        val seenWhileCompiling = guard.compiling(material) {
            every { prefs.all } returns mapOf(key to true)
            guard.crashed(material)
        }

        assertFalse(seenWhileCompiling)
    }
}
