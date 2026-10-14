package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Batched storage reads drop keys the chain holds nothing for, so a complete response and a partial one
 * arrive looking the same. Restoring them is what lets a reader use `getValue` and have a violated
 * expectation surface as an exception rather than as a plausible value.
 */
class EnsureKeysWithNullDefaultTest {
    @Test
    fun `a key the map has no entry for comes back as null`() {
        val response = mapOf("present" to 1)

        val completed = response.ensureKeysWithNullDefault(listOf("present", "dropped"))

        assertEquals(setOf("present", "dropped"), completed.keys)
        assertEquals(1, completed.getValue("present"))
        assertEquals(null, completed.getValue("dropped"))
    }

    @Test
    fun `a key nobody asked about is dropped rather than carried through`() {
        val response = mapOf("asked" to 1, "unasked" to 2)

        val completed = response.ensureKeysWithNullDefault(listOf("asked"))

        assertEquals(setOf("asked"), completed.keys)
    }

    @Test
    fun `an empty map still answers for every requested key`() {
        val completed = emptyMap<String, Int>().ensureKeysWithNullDefault(listOf("a", "b"))

        assertEquals(setOf("a", "b"), completed.keys)
        assertTrue(completed.values.all { it == null })
    }

    @Test
    fun `a repeated key is asked about once`() {
        val completed = mapOf("a" to 1).ensureKeysWithNullDefault(listOf("a", "a"))

        assertEquals(1, completed.size)
    }
}
