package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import kotlinx.collections.immutable.persistentListOf
import org.junit.Assert.assertEquals
import org.junit.Test

class CommandCatalogueTest {
    private val forecast = ChatCommandModel("forecast", "The forecast", argumentHint = "<city>")
    private val fog = ChatCommandModel("fog", "Fog alerts", argumentHint = null)
    private val rain = ChatCommandModel("rain", "Rain alerts", argumentHint = null)
    private val commands = persistentListOf(forecast, fog, rain)

    @Test
    fun `a slash alone opens the whole catalogue`() {
        assertEquals(commands, commands.catalogueFor("/"))
    }

    @Test
    fun `the name typed after the slash filters by prefix`() {
        assertEquals(listOf(forecast, fog), commands.catalogueFor("/fo"))
        assertEquals(listOf(forecast), commands.catalogueFor("/FORE"))
    }

    @Test
    fun `text that is not a command name opens nothing`() {
        assertEquals(emptyList<ChatCommandModel>(), commands.catalogueFor("hello"))
        assertEquals(emptyList<ChatCommandModel>(), commands.catalogueFor("/forecast Berlin"))
        assertEquals(emptyList<ChatCommandModel>(), commands.catalogueFor(""))
    }

    @Test
    fun `choosing a command puts its name and a space in the input`() {
        assertEquals("/forecast ", forecast.inputText())
    }
}
