package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud

import android.content.Context
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import timber.log.Timber
import java.util.Locale
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

/**
 * "Read aloud" of a product room (07-LATER §9.5): the host speaks the text of one message with the text to speech
 * engine of the system, in the locale of the host. One message speaks at a time, and a new one stops the last one.
 */
interface ChatReadAloudController {
    /** The room whose message speaks now, or null. */
    val speaking: StateFlow<ChatId?>

    fun speak(chatId: ChatId, text: String)

    /** Stops the speech when it is for [chatId]. */
    fun stop(chatId: ChatId)
}

@Singleton
class TextToSpeechReadAloudController @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : ChatReadAloudController {
    private val _speaking = MutableStateFlow<ChatId?>(null)
    override val speaking: StateFlow<ChatId?> = _speaking.asStateFlow()

    private val lock = Any()
    private var engine: TextToSpeech? = null
    private var ready = false
    private var pending: Utterance? = null
    private var current: Utterance? = null

    private class Utterance(val chatId: ChatId, val text: String, val id: String = UUID.randomUUID().toString())

    override fun speak(chatId: ChatId, text: String) {
        val utterance = Utterance(chatId, text)
        // The launcher of the emulator reads this line: the length, and whether Markdown marks are left (S67).
        Timber.i("truapi.chat.readAloud speak %d characters, marks %b", text.length, text.any { it in MARKDOWN_MARKS })
        synchronized(lock) {
            current = utterance
            _speaking.value = chatId
            val tts = engine ?: createEngine().also { engine = it }
            if (ready) start(tts, utterance) else pending = utterance
        }
    }

    override fun stop(chatId: ChatId) {
        synchronized(lock) {
            if (current?.chatId != chatId) return
            Timber.i("truapi.chat.readAloud stop")
            current = null
            pending = null
            _speaking.value = null
            engine?.stop()
        }
    }

    private fun createEngine(): TextToSpeech {
        lateinit var created: TextToSpeech
        created = TextToSpeech(context) { status -> onInit(created, status) }
        created.setOnUtteranceProgressListener(ProgressListener())
        return created
    }

    private fun onInit(tts: TextToSpeech, status: Int) = synchronized(lock) {
        if (status != TextToSpeech.SUCCESS) {
            Timber.w("truapi.chat.readAloud unavailable: the engine did not start (%d)", status)
            engine = null
            pending = null
            current = null
            _speaking.value = null
            tts.shutdown()
            return@synchronized
        }
        ready = true
        tts.language = Locale.getDefault()
        pending?.let { start(tts, it) }
        pending = null
    }

    private fun start(tts: TextToSpeech, utterance: Utterance) {
        // The engine takes at most `getMaxSpeechInputLength` characters for one utterance.
        val text = utterance.text.take(TextToSpeech.getMaxSpeechInputLength())
        val result = tts.speak(text, TextToSpeech.QUEUE_FLUSH, null, utterance.id)
        if (result != TextToSpeech.SUCCESS) finish(utterance.id, "the engine refused the text")
    }

    private fun finish(utteranceId: String?, problem: String?) = synchronized(lock) {
        if (current?.id != utteranceId) return@synchronized
        if (problem == null) Timber.i("truapi.chat.readAloud done") else Timber.w("truapi.chat.readAloud failed: %s", problem)
        current = null
        _speaking.value = null
    }

    private inner class ProgressListener : UtteranceProgressListener() {
        override fun onStart(utteranceId: String?) = Unit

        override fun onDone(utteranceId: String?) = finish(utteranceId, problem = null)

        @Deprecated("Deprecated in Java")
        override fun onError(utteranceId: String?) = finish(utteranceId, problem = "the engine failed")

        override fun onError(utteranceId: String?, errorCode: Int) = finish(utteranceId, problem = "the engine failed ($errorCode)")
    }

    private companion object {
        val MARKDOWN_MARKS = setOf('*', '`')
    }
}
