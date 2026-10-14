package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SlashCommandTest {
    @Test
    fun `a command splits its name from its arguments`() {
        val command = SlashCommand.parse("/roll 2 dice")

        assertEquals("roll", command?.name)
        assertEquals("2 dice", command?.arguments)
    }

    @Test
    fun `a command without arguments has empty arguments`() {
        assertEquals("", SlashCommand.parse("/help")?.arguments)
    }

    @Test
    fun `text that is no command is not parsed`() {
        assertNull(SlashCommand.parse("hello /roll"))
        assertNull(SlashCommand.parse("/ roll"))
        assertNull(SlashCommand.parse("/"))
    }
}
