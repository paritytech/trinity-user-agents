package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.content.SharedPreferences
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
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

    // The marker must be on disk before the compile starts, or a compile that kills the process leaves no trace.
    @Test
    fun `a compile is marked before it runs and unmarked after`() {
        val compiled = CompileGuard(prefsHolding()).compiling(material) { 42 }

        assertEquals(42, compiled)
        verifyOrder {
            editor.putBoolean(key, true)
            editor.commit()
            editor.remove(key)
            editor.commit()
        }
    }

    @Test
    fun `a compile that throws is still unmarked`() {
        assertThrows(IllegalStateException::class.java) {
            CompileGuard(prefsHolding()).compiling(material) { error("compile failed") }
        }

        verify { editor.remove(key) }
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
