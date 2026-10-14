package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

/** A `/name arguments` line that the user typed. */
class SlashCommand(val name: String, val arguments: String) {
    companion object {
        fun parse(text: String): SlashCommand? {
            if (!text.startsWith("/")) return null
            val body = text.removePrefix("/")
            val name = body.substringBefore(' ').substringBefore('\n')
            if (name.isEmpty()) return null
            return SlashCommand(name, body.removePrefix(name).trim())
        }
    }
}
