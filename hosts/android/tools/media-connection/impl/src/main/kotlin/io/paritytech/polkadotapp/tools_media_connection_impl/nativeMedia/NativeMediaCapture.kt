package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.projection.MediaProjection
import android.os.Build
import androidx.core.content.ContextCompat
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import io.paritytech.polkadotapp.tools_media_connection_impl.AudioCaptureState
import kotlinx.coroutines.CompletableDeferred
import org.webrtc.*
import uniffi.truapi.*

internal class MediaDomainFailure(val domain: NativeMediaDomainError) : Exception()

/** The prepared tracks are disabled until the core's CommitOperation. */
internal class NativeMediaCapture(private val context: Context, private val core: WebRtcCore) : AutoCloseable {
    var microphone: AudioTrack? = null
        private set
    var camera: VideoTrack? = null
        private set
    var screen: VideoTrack? = null
        private set
    var cameraKind: NativeMediaCameraKind? = null
        private set
    private var audioResource: AudioResources? = null
    private val videoResources = mutableListOf<VideoResources>()
    private var closed = false
    private class VideoResources(val track: VideoTrack, val source: VideoSource, val capturer: VideoCapturer, val helper: SurfaceTextureHelper) {
        var references = 1
        @Volatile var live = false
        @Volatile var failed = false
        var wasLive = false
    }
    private class AudioResources(val track: AudioTrack, val source: AudioSource) {
        var references = 1
        var wasLive = false
    }

    fun reuse(previous: NativeMediaCapture?, intent: NativeMediaLocalTracks) {
        if (previous == null) return
        if (intent.microphone) previous.audioResource?.let { resource ->
            resource.references++
            audioResource = resource
            microphone = resource.track
        }
        fun borrow(track: VideoTrack?): VideoTrack? {
            val resource = previous.videoResources.firstOrNull { it.track === track } ?: return null
            resource.references++
            videoResources.add(resource)
            return resource.track
        }
        if (intent.camera) { camera = borrow(previous.camera); cameraKind = previous.cameraKind }
        if (intent.screen) screen = borrow(previous.screen)
    }

    fun microphone() {
        checkPermission(Manifest.permission.RECORD_AUDIO)
        val source = core.peerConnectionFactory.createAudioSource(MediaConstraints())
        val track = core.peerConnectionFactory.createAudioTrack("microphone", source).also { it.setEnabled(false) }
        audioResource = AudioResources(track, source)
        microphone = track
    }

    suspend fun camera(preference: NativeMediaCameraKind?, changed: () -> Unit) {
        checkPermission(Manifest.permission.CAMERA)
        val enumerator = Camera2Enumerator(context)
        val id = enumerator.deviceNames.firstOrNull {
            if (preference == NativeMediaCameraKind.REAR) enumerator.isBackFacing(it) else enumerator.isFrontFacing(it)
        } ?: enumerator.deviceNames.firstOrNull() ?: throw MediaDomainFailure(NativeMediaDomainError.DeviceUnavailable)
        cameraKind = when { enumerator.isFrontFacing(id) -> NativeMediaCameraKind.FRONT; enumerator.isBackFacing(id) -> NativeMediaCameraKind.REAR; else -> NativeMediaCameraKind.OTHER }
        val started = CompletableDeferred<Unit>()
        var selectedResource: VideoResources? = null
        fun interrupted() {
            android.os.Handler(context.mainLooper).post {
                selectedResource?.let { it.live = false; it.failed = true }
                started.completeExceptionally(MediaDomainFailure(NativeMediaDomainError.DeviceUnavailable))
                changed()
            }
        }
        val capturer = enumerator.createCapturer(id, object : CameraVideoCapturer.CameraEventsHandler {
            override fun onCameraError(errorDescription: String?) = interrupted()
            override fun onCameraDisconnected() = interrupted()
            override fun onCameraFreezed(errorDescription: String?) = interrupted()
            override fun onCameraOpening(cameraName: String?) {}
            override fun onFirstFrameAvailable() = changed()
            override fun onCameraClosed() = interrupted()
        }) ?: throw MediaDomainFailure(NativeMediaDomainError.DeviceUnavailable)
        camera = video("camera", capturer, false, changed, started)
        selectedResource = videoResources.first { it.track === camera }
        started.await()
    }

    suspend fun screen(stopped: (VideoTrack?) -> Unit, startProjectionService: suspend () -> Unit, changed: () -> Unit) {
        val consent = NativeMediaConsent.screen(context) ?: throw MediaDomainFailure(NativeMediaDomainError.CaptureCancelled)
        if (closed) throw MediaDomainFailure(NativeMediaDomainError.OperationCancelled)
        startProjectionService()
        if (closed) throw MediaDomainFailure(NativeMediaDomainError.OperationCancelled)
        var selectedTrack: VideoTrack? = null
        val capturer = ScreenCapturerAndroid(consent, object : MediaProjection.Callback() {
            override fun onStop() { stopped(selectedTrack) }
        })
        val started = CompletableDeferred<Unit>()
        screen = video("screen", capturer, true, changed, started).also { selectedTrack = it }
        started.await()
    }

    private fun video(id: String, capturer: VideoCapturer, screencast: Boolean, changed: () -> Unit, started: CompletableDeferred<Unit>): VideoTrack {
        val helper = SurfaceTextureHelper.create("native-media-$id", core.eglBase.eglBaseContext)
        val source = core.peerConnectionFactory.createVideoSource(screencast)
        val track = core.peerConnectionFactory.createVideoTrack(id, source)
        val resource = VideoResources(track, source, capturer, helper)
        videoResources.add(resource)
        track.setEnabled(false)
        capturer.initialize(helper, context, object : CapturerObserver {
            override fun onCapturerStarted(success: Boolean) {
                source.capturerObserver.onCapturerStarted(success)
                resource.live = false
                resource.failed = !success
                if (success) started.complete(Unit) else started.completeExceptionally(MediaDomainFailure(NativeMediaDomainError.DeviceUnavailable))
                changed()
            }
            override fun onCapturerStopped() { resource.live = false; source.capturerObserver.onCapturerStopped(); changed() }
            override fun onFrameCaptured(frame: VideoFrame) {
                source.capturerObserver.onFrameCaptured(frame)
                if (!resource.live) { resource.live = true; resource.failed = false; changed() }
            }
        })
        val metrics = context.resources.displayMetrics
        capturer.startCapture(if (screencast) metrics.widthPixels else 1280, if (screencast) metrics.heightPixels else 720, if (screencast) 15 else 30)
        return track
    }

    fun tracks(): List<MediaStreamTrack?> = listOf(microphone, camera, screen)
    fun enable() { tracks().forEach { if (it != null) check(it.setEnabled(true)) } }
    fun disable() { tracks().forEach { if (it != null) check(it.setEnabled(false)) } }
    fun stopScreen() {
        val track = screen ?: return
        screen = null
        videoResources.firstOrNull { it.track === track }?.let { resource ->
            videoResources.remove(resource)
            release(resource)
        }
    }

    fun state(route: NativeMediaAudioRoute?): NativeMediaLocalState = NativeMediaLocalState(
        state(microphone), state(camera), state(screen), cameraKind, route,
    )

    private fun state(track: MediaStreamTrack?): NativeMediaTrackState {
        if (track == null) return NativeMediaTrackState.OFF
        if (track.state() == MediaStreamTrack.State.ENDED) return NativeMediaTrackState.INTERRUPTED
        if (track is AudioTrack) {
            val audio = audioResource ?: return NativeMediaTrackState.OFF
            val live = track.enabled() && core.audioCaptureState.value == AudioCaptureState.LIVE
            if (live) audio.wasLive = true
            return if (live) NativeMediaTrackState.LIVE else if (audio.wasLive || core.audioCaptureState.value == AudioCaptureState.FAILED) NativeMediaTrackState.INTERRUPTED else NativeMediaTrackState.STARTING
        }
        val resource = videoResources.firstOrNull { it.track === track } ?: return NativeMediaTrackState.OFF
        val live = track.enabled() && resource.live
        if (live) resource.wasLive = true
        return if (live) NativeMediaTrackState.LIVE else if (resource.wasLive || resource.failed) NativeMediaTrackState.INTERRUPTED else NativeMediaTrackState.STARTING
    }

    override fun close() {
        if (closed) return
        closed = true
        videoResources.toList().forEach { runCatching { release(it) } }
        videoResources.clear()
        audioResource?.let { resource ->
            if (--resource.references == 0) {
                runCatching { resource.track.setEnabled(false) }
                runCatching { resource.track.dispose() }
                runCatching { resource.source.dispose() }
            }
        }
        audioResource = null
        microphone = null; camera = null; screen = null
    }

    private fun release(resource: VideoResources) {
        if (--resource.references != 0) return
        runCatching { resource.track.setEnabled(false) }
        runCatching { resource.capturer.stopCapture() }
        runCatching { resource.capturer.dispose() }
        runCatching { resource.track.dispose() }
        runCatching { resource.source.dispose() }
        runCatching { resource.helper.dispose() }
    }

    private fun checkPermission(permission: String) {
        if (ContextCompat.checkSelfPermission(context, permission) != PackageManager.PERMISSION_GRANTED) throw SecurityException()
    }
}

internal class NativeMediaAudio(context: Context, private val focusLost: () -> Unit) : AutoCloseable {
    private val manager = context.getSystemService(AudioManager::class.java)
    private var active = false

    fun select(preference: NativeMediaAudioRoute?): NativeMediaAudioRoute {
        if (!active) {
            if (owners.isEmpty()) {
                previousMode = manager.mode
                previousSpeaker = manager.isSpeakerphoneOn
                val request = android.media.AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                    .setAudioAttributes(android.media.AudioAttributes.Builder()
                        .setUsage(android.media.AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(android.media.AudioAttributes.CONTENT_TYPE_SPEECH).build())
                    .setOnAudioFocusChangeListener({ change ->
                        if (change == AudioManager.AUDIOFOCUS_LOSS) owners.values.toList().forEach { it() }
                    }, android.os.Handler(android.os.Looper.getMainLooper()))
                    .build()
                if (manager.requestAudioFocus(request) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
                    throw MediaDomainFailure(NativeMediaDomainError.DeviceUnavailable)
                }
                focus = request
            }
            owners[this] = focusLost
            active = true
        }
        manager.mode = AudioManager.MODE_IN_COMMUNICATION
        if (Build.VERSION.SDK_INT >= 31) {
            val devices = manager.availableCommunicationDevices
            val preferred = devices.firstOrNull { classify(it) == preference }
                ?: devices.firstOrNull { classify(it) == NativeMediaAudioRoute.HEADSET }
            if (preferred != null) manager.setCommunicationDevice(preferred)
        } else {
            @Suppress("DEPRECATION")
            when (preference) {
                NativeMediaAudioRoute.SPEAKER -> manager.isSpeakerphoneOn = true
                NativeMediaAudioRoute.EARPIECE -> manager.isSpeakerphoneOn = false
                NativeMediaAudioRoute.HEADSET -> if (manager.isBluetoothScoAvailableOffCall) {
                    manager.startBluetoothSco()
                    manager.isBluetoothScoOn = true
                }
                else -> Unit
            }
        }
        return current()
    }

    @Suppress("DEPRECATION")
    fun current(): NativeMediaAudioRoute = if (Build.VERSION.SDK_INT >= 31) {
        manager.communicationDevice?.let(::classify) ?: NativeMediaAudioRoute.OTHER
    } else when {
        manager.isSpeakerphoneOn -> NativeMediaAudioRoute.SPEAKER
        manager.isBluetoothScoOn || manager.isWiredHeadsetOn -> NativeMediaAudioRoute.HEADSET
        else -> NativeMediaAudioRoute.EARPIECE
    }

    private fun classify(device: AudioDeviceInfo) = when (device.type) {
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> NativeMediaAudioRoute.EARPIECE
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> NativeMediaAudioRoute.SPEAKER
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET,
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES, AudioDeviceInfo.TYPE_WIRED_HEADSET,
        AudioDeviceInfo.TYPE_USB_HEADSET -> NativeMediaAudioRoute.HEADSET
        else -> NativeMediaAudioRoute.OTHER
    }

    override fun close() {
        if (!active) return
        active = false
        owners.remove(this)
        if (owners.isNotEmpty()) return
        if (Build.VERSION.SDK_INT >= 31) manager.clearCommunicationDevice() else manager.stopBluetoothSco()
        focus?.let(manager::abandonAudioFocusRequest)
        focus = null
        manager.isSpeakerphoneOn = previousSpeaker
        manager.mode = previousMode
    }

    private companion object {
        // All leases execute on Main. Restore global routing only after the
        // final product runtime releases it, not when an arbitrary view closes.
        val owners = mutableMapOf<NativeMediaAudio, () -> Unit>()
        var focus: android.media.AudioFocusRequest? = null
        var previousMode = AudioManager.MODE_NORMAL
        var previousSpeaker = false
    }
}
