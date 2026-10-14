package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class MapValuesDiffedTest {
    private val transformed = mutableListOf<Pair<String, Int>>()

    @Test
    fun `every entry is transformed on the first emission`() = runBlocking<Unit> {
        val results = flowOf(mapOf("a" to 1, "b" to 2)).mapValuesDiffed(::record).toList()

        assertEquals(listOf(mapOf("a" to "1", "b" to "2")), results)
        assertEquals(listOf("a" to 1, "b" to 2), transformed)
    }

    @Test
    fun `only the entries whose value changed are transformed again`() = runBlocking<Unit> {
        val results = flowOf(
            mapOf("a" to 1, "b" to 2),
            mapOf("a" to 1, "b" to 3),
        ).mapValuesDiffed(::record).toList()

        assertEquals(mapOf("a" to "1", "b" to "3"), results.last())
        assertEquals(listOf("a" to 1, "b" to 2, "b" to 3), transformed)
    }

    @Test
    fun `an entry absent from an emission is dropped and transformed anew when it returns`() = runBlocking<Unit> {
        val results = flowOf(
            mapOf("a" to 1, "b" to 2),
            mapOf("a" to 1),
            mapOf("a" to 1, "b" to 2),
        ).mapValuesDiffed(::record).toList()

        assertEquals(
            listOf(mapOf("a" to "1", "b" to "2"), mapOf("a" to "1"), mapOf("a" to "1", "b" to "2")),
            results,
        )
        assertEquals(listOf("a" to 1, "b" to 2, "b" to 2), transformed)
    }

    @Test
    fun `an unchanged entry keeps its previous result even when the value is null`() = runBlocking<Unit> {
        val results = flowOf(
            mapOf("a" to null),
            mapOf("a" to null),
        ).mapValuesDiffed { key, _ -> key.uppercase() }.toList()

        assertEquals(listOf(mapOf("a" to "A"), mapOf("a" to "A")), results)
    }

    private fun record(key: String, value: Int): String {
        transformed += key to value

        return value.toString()
    }
}
