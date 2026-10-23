package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.tools_media_connection_impl.AudioCaptureState
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.webrtc.Camera2Enumerator
import org.webrtc.VideoSink
import org.webrtc.VideoTrack
import uniffi.truapi.NativeMediaCameraKind
import uniffi.truapi.NativeMediaRemoteState
import uniffi.truapi.NativeMediaTrackState
import uniffi.truapi.NativeMediaBackendEvent
import uniffi.truapi.NativeMediaBackendPeerState
import uniffi.truapi.NativeMediaDescriptionKind

class MediaRegressionActivity : Activity()

@RunWith(AndroidJUnit4::class)
class NativeMediaPeerInstrumentedTest {
    @Test
    fun receiveOnlyAnswererConnectsThenSendsCameraAcrossRenegotiations(): Unit = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val initialCameraPermission = context.checkSelfPermission(Manifest.permission.CAMERA)
        val initialMicrophonePermission = context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
        val activity = instrumentation.startActivitySync(
            Intent(context, MediaRegressionActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            withTimeout(120_000) {
                withContext(Dispatchers.Main.immediate) {
                    coroutineScope {
                        val core = WebRtcCore(context)
                        val capture = NativeMediaCapture(context, core)
                        val events = Channel<Pair<Boolean, NativeMediaBackendEvent>>(Channel.UNLIMITED)
                        val connected = mutableSetOf<Boolean>()
                        val remote = mutableMapOf<Boolean, NativeMediaRemoteState>()
                        val frames = AtomicInteger()
                        var receiving: VideoTrack? = null
                        var closing = false
                        var answers = 0
                        var offersFromAnswerer = 0
                        val sink = VideoSink { frame ->
                            if (frame.rotatedWidth > 0 && frame.rotatedHeight > 0) frames.incrementAndGet()
                        }
                        lateinit var caller: NativeMediaPeer
                        lateinit var answerer: NativeMediaPeer
                        fun refreshPicture() {
                            val next = if (closing) null else caller.picture(false)
                            if (receiving !== next) {
                                receiving?.removeSink(sink)
                                receiving = next
                                receiving?.addSink(sink)
                            }
                        }
                        fun peer(offerer: Boolean) = NativeMediaPeer(
                            core = core,
                            servers = emptyList(),
                            initialOfferer = offerer,
                            scope = this,
                            emit = { events.trySend(offerer to it) },
                            sessionId = ByteArray(32) { 1 },
                            participantId = ByteArray(32) { if (offerer) 2 else 3 },
                            picturesChanged = { if (offerer) refreshPicture() },
                            released = {},
                        )
                        caller = peer(true)
                        answerer = peer(false)
                        val relay = launch {
                            for ((fromCaller, event) in events) {
                                val destination = if (fromCaller) answerer else caller
                                when (event) {
                                    is NativeMediaBackendEvent.Description -> {
                                        assertEquals(
                                            "Renegotiation must preserve exactly microphone/camera/screen m-lines",
                                            listOf("audio", "video", "video"),
                                            event.description.sdp.lineSequence().filter { it.startsWith("m=") }
                                                .map { it.substringAfter("m=").substringBefore(' ') }.toList(),
                                        )
                                        if (event.description.kind == NativeMediaDescriptionKind.ANSWER) answers++
                                        if (!fromCaller && event.description.kind == NativeMediaDescriptionKind.OFFER) offersFromAnswerer++
                                        destination.description(event.description)
                                    }
                                    is NativeMediaBackendEvent.IceCandidate -> destination.candidate(event.candidate)
                                    is NativeMediaBackendEvent.PeerStateChanged -> {
                                        check(event.state != NativeMediaBackendPeerState.FAILED) {
                                            "Native ${if (fromCaller) "caller" else "answerer"} failed"
                                        }
                                        if (event.state == NativeMediaBackendPeerState.CONNECTED) connected.add(fromCaller)
                                    }
                                    is NativeMediaBackendEvent.RemoteStateChanged -> remote[fromCaller] = event.state
                                    else -> error("Unexpected native peer event: ${event.javaClass.simpleName}")
                                }
                            }
                        }
                        try {
                            caller.setTracks(listOf(null, null, null))
                            answerer.setTracks(listOf(null, null, null))
                            answerer.start()
                            caller.start()
                            await("Both receive-only peers must reach CONNECTED without acquiring capture") {
                                connected.size == 2 && answers >= 1 && remote.size == 2
                            }
                            remote.values.forEach {
                                assertEquals(NativeMediaTrackState.OFF, it.microphone)
                                assertEquals(NativeMediaTrackState.OFF, it.camera)
                                assertEquals(NativeMediaTrackState.OFF, it.screen)
                            }
                            assertEquals(AudioCaptureState.IDLE, core.audioCaptureState.value)
                            assertNull(capture.microphone)
                            assertNull(capture.camera)
                            assertNull(capture.screen)
                            assertEquals(initialCameraPermission, context.checkSelfPermission(Manifest.permission.CAMERA))
                            assertEquals(initialMicrophonePermission, context.checkSelfPermission(Manifest.permission.RECORD_AUDIO))
                            assertEquals(0, frames.get())
                            Log.i(TAG, "receive-only CONNECTED on both peers; no capture or device grant changes")

                            withContext(Dispatchers.IO) {
                                instrumentation.uiAutomation.grantRuntimePermission(context.packageName, Manifest.permission.CAMERA)
                            }
                            assertTrue("A real Camera2 device is required; configure an owned emulator camera", Camera2Enumerator.isSupported(context))
                            assertFalse("No emulator/hardware camera was enumerated", Camera2Enumerator(context).deviceNames.isEmpty())
                            capture.camera(NativeMediaCameraKind.FRONT) {}
                            capture.enable()
                            val previousAnswers = answers
                            answerer.setTracks(capture.tracks())
                            answerer.renegotiateLater()
                            await("Answerer camera must renegotiate and deliver decoded remote frames") {
                                answers > previousAnswers && offersFromAnswerer >= 1 &&
                                    capture.state(null).camera == NativeMediaTrackState.LIVE &&
                                    remote[true]?.camera == NativeMediaTrackState.LIVE && frames.get() >= 3
                            }
                            assertEquals(NativeMediaTrackState.OFF, remote[true]?.microphone)
                            assertEquals(NativeMediaTrackState.OFF, remote[true]?.screen)
                            Log.i(TAG, "answerer camera LIVE; decoded remote frames=${frames.get()}")

                            val beforeDisable = answers
                            capture.disable()
                            answerer.setTracks(listOf(null, null, null))
                            answerer.renegotiateLater()
                            await("Camera removal must detach the receiver sink after renegotiation") {
                                answers > beforeDisable && remote[true]?.camera == NativeMediaTrackState.OFF && receiving == null
                            }
                            val beforeResume = answers
                            val previousFrames = frames.get()
                            capture.enable()
                            answerer.setTracks(capture.tracks())
                            answerer.renegotiateLater()
                            await("Reacquired receiver wrappers must deliver frames after a second camera enable") {
                                answers > beforeResume && remote[true]?.camera == NativeMediaTrackState.LIVE && frames.get() >= previousFrames + 3
                            }
                            assertEquals(initialMicrophonePermission, context.checkSelfPermission(Manifest.permission.RECORD_AUDIO))
                            assertEquals(AudioCaptureState.IDLE, core.audioCaptureState.value)
                            Log.i(TAG, "camera OFF then LIVE again; decoded remote frames=${frames.get()}; microphone stayed idle")
                        } finally {
                            withContext(NonCancellable) {
                                closing = true
                                refreshPicture()
                                caller.close()
                                answerer.close()
                                relay.cancelAndJoin()
                                events.close()
                                capture.close()
                                core.peerConnectionFactory.dispose()
                                core.eglBase.release()
                            }
                        }
                    }
                }
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private suspend fun await(message: String, condition: () -> Boolean) {
        try {
            withTimeout(30_000) { while (!condition()) delay(25) }
        } catch (failure: kotlinx.coroutines.TimeoutCancellationException) {
            throw AssertionError(message, failure)
        }
    }

    companion object {
        private const val TAG = "NativeMediaRegression"
    }
}
