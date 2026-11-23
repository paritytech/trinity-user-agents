package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.Manifest
import android.app.AppOpsManager
import android.content.Context
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.ExternalRtcConfigProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.toIceServers
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.webrtc.PeerConnection
import uniffi.truapi.*
import uniffi.truapi.ProductContext
import uniffi.truapi.NativeMediaCallbacks
import uniffi.truapi.NativeMediaEventSink
import uniffi.truapi.NativeMediaException
import uniffi.truapi.MediaBackendCapabilities
import uniffi.truapi.MediaBackendResponse
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@Singleton
class NativeMediaBackendFactory @Inject internal constructor(
    @param:ApplicationContext private val context: Context,
    private val core: WebRtcCore,
    private val rtcConfig: ExternalRtcConfigProvider,
) {
    fun create(productId: String): NativeMediaBackend = NativeMediaBackend(context, core, rtcConfig, productId)
}

/** One independently authenticated native product execution, including Worker executions with no view. */
class NativeMediaBackend internal constructor(
    private val context: Context,
    private val core: WebRtcCore,
    private val rtcConfig: ExternalRtcConfigProvider,
    private val productId: String,
) : NativeMediaCallbacks, AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate + CoroutineExceptionHandler { _, _ ->
        runtimes.values.toList().forEach { runtime ->
            runtime.sessions.values.toList().forEach { emit(runtime, MediaBackendEvent.HostEnded(it.id)) }
        }
        close()
    })
    private val runtimes = mutableMapOf<ULong, Runtime>()
    private val closedRuntimes = java.util.concurrent.ConcurrentHashMap.newKeySet<ULong>()
    @Volatile private var closed = false
    private var view: WebView? = null

    private inner class Runtime(val id: ULong, var sink: NativeMediaEventSink?) {
        val sessions = mutableMapOf<String, Session>()
        val operations = mutableMapOf<String, Pending>()
        val cancelled = mutableSetOf<String>()
        val consentJobs = mutableMapOf<String, Job>()
        val audio = NativeMediaAudio(context) {
            sessions.values.toList().forEach { session ->
                end(session)
                emit(this, MediaBackendEvent.HostEnded(session.id))
            }
        }
        val compositor = NativeMediaCompositor(core.eglBase.eglBaseContext,
            { emit(this, MediaBackendEvent.ViewportChanged(it)) },
            { session, source -> sessions[session]?.picture(source) })
        var closed = false
    }

    private inner class Session(val runtime: Runtime, val id: ByteArray) {
        val key = id.key()
        val lease = "${System.identityHashCode(this@NativeMediaBackend)}:${runtime.id}:$key"
        var revision = 0uL
        var generation = 0uL
        var capture: NativeMediaCapture? = null
        var committed = false
        var screenStopped = false
        var tracks = MediaLocalTracks(false, false, false, null, null)
        val peers = mutableMapOf<String, NativeMediaPeer>()
        fun picture(source: MediaPictureSource) = when (source) {
            is MediaPictureSource.Local -> capture?.let { local ->
                val state = local.state(runtime.audio.current())
                if (source.picture == MediaPictureKind.SCREEN) local.screen.takeIf { state.screen == MediaTrackState.LIVE }
                else local.camera.takeIf { state.camera == MediaTrackState.LIVE }
            }
            is MediaPictureSource.Remote -> peers[source.participantId.key()]?.picture(source.picture == MediaPictureKind.SCREEN)
        }
    }

    private class Pending(val sessionKey: String, val generation: ULong, val revision: ULong, val tracks: MediaLocalTracks, val capture: NativeMediaCapture, val job: Job) {
        var ready = false
        var foregroundTypes = 0
        // An initial OS consent prompt is not a withdrawal. Remember grants before capture exists.
        var microphoneAuthorized = false
        var cameraAuthorized = false
    }

    private val appOps = context.getSystemService(AppOpsManager::class.java)
    private val permissionsListener = AppOpsManager.OnOpChangedListener { _, packageName ->
        if (packageName == null || packageName == context.packageName) scope.launch { checkPermissions() }
    }
    private val audioListener = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>?) = audioChanged()
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>?) = audioChanged()
    }

    init {
        appOps.startWatchingMode(AppOpsManager.OPSTR_CAMERA, context.packageName, permissionsListener)
        appOps.startWatchingMode(AppOpsManager.OPSTR_RECORD_AUDIO, context.packageName, permissionsListener)
        context.getSystemService(AudioManager::class.java).registerAudioDeviceCallback(audioListener, Handler(Looper.getMainLooper()))
        scope.launch { core.audioCaptureState.collect { audioChanged() } }
        scope.launch {
            while (isActive) {
                delay(1_000)
                if (!NativeMediaService.indicatorAvailable(context)) {
                    runtimes.values.toList().forEach { runtime ->
                        runtime.sessions.values.toList().forEach { session ->
                            end(session)
                            emit(runtime, MediaBackendEvent.HostEnded(session.id))
                        }
                    }
                }
                checkPermissions()
            }
        }
    }

    override suspend fun capabilities(product: ProductContext): MediaBackendCapabilities = withContext(Dispatchers.Main.immediate) {
        owner(product)
        val supported = !closed && NativeMediaService.indicatorAvailable(context) && runCatching {
            core.eglBase.eglBaseContext
            core.peerConnectionFactory
        }.isSuccess
        MediaBackendCapabilities(supported, 1u, 5u, 12u)
    }

    override fun subscribe(product: ProductContext, runtimeId: ULong, eventSink: NativeMediaEventSink) {
        owner(product)
        if (closed || runtimeId in closedRuntimes) throw NativeMediaException.Closed()
        scope.launch {
            if (closed || runtimeId in closedRuntimes) return@launch
            val runtime = runtimes.getOrPut(runtimeId) { Runtime(runtimeId, eventSink) }
            runtime.sink = eventSink
            view?.let(runtime.compositor::attach)
            emit(runtime, MediaBackendEvent.ViewportChanged(runtime.compositor.current()))
        }
    }

    override fun unsubscribe(runtimeId: ULong) {
        scope.launch { runtimes[runtimeId]?.sink = null }
    }

    override suspend fun command(product: ProductContext, runtimeId: ULong, command: MediaBackendCommand): MediaBackendResponse = withContext(Dispatchers.Main.immediate) {
        owner(product)
        if (closed) throw NativeMediaException.Closed()
        if (runtimeId in closedRuntimes) {
            if (command == MediaBackendCommand.CloseRuntime) return@withContext MediaBackendResponse.Done
            throw NativeMediaException.Closed()
        }
        val runtime = runtimes.getOrPut(runtimeId) { Runtime(runtimeId, null) }
        try {
            when (command) {
                is MediaBackendCommand.RequestConsent -> consent(runtime, command)
                is MediaBackendCommand.OpenSession -> {
                    if (command.operationId.key() in runtime.cancelled) throw MediaDomainFailure(HostMediaError.OperationCancelled)
                    if (runtime.sessions.isNotEmpty()) throw MediaDomainFailure(HostMediaError.CapacityExceeded(5u))
                    val session = Session(runtime, command.sessionId.copyOf())
                    runtime.sessions[session.key] = session
                    prepare(runtime, session, command.operationId, 0u, command.tracks)
                }
                is MediaBackendCommand.SetTracks -> prepare(runtime, session(runtime, command.sessionId), command.operationId, command.intentRevision, command.tracks)
                is MediaBackendCommand.CommitOperation -> commit(runtime, command.operationId)
                is MediaBackendCommand.CancelOperation -> {
                    cancel(runtime, command.operationId.key())
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.CloseSession -> {
                    runtime.sessions[command.sessionId.key()]?.let(::end)
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.CreatePeer -> {
                    val session = session(runtime, command.sessionId)
                    if (!session.committed) throw MediaDomainFailure(HostMediaError.InvalidState)
                    if (session.peers.size >= 5) throw MediaDomainFailure(HostMediaError.CapacityExceeded(5u))
                    if (session.peers.containsKey(command.participantId.key())) throw MediaDomainFailure(HostMediaError.InvalidState)
                    val servers = nativeMediaIceServers(rtcConfig::getConfig)
                    if (runtime.closed || runtime.sessions[session.key] !== session) throw MediaDomainFailure(HostMediaError.SessionEnded)
                    val peer = NativeMediaPeer(core, servers, command.offerer, scope, { emit(runtime, it) }, session.id,
                        command.participantId.copyOf(), runtime.compositor::refresh, { session.peers.remove(command.participantId.key()) })
                    session.peers[command.participantId.key()] = peer
                    peer.setTracks(session.capture?.tracks() ?: listOf(null, null, null))
                    peer.start()
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.ApplyDescription -> {
                    peer(runtime, command.sessionId, command.participantId).description(command.description)
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.AddIceCandidate -> {
                    peer(runtime, command.sessionId, command.participantId).candidate(command.candidate)
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.RemovePeer -> {
                    runtime.sessions[command.sessionId.key()]?.peers?.remove(command.participantId.key())?.close()
                    runtime.compositor.refresh()
                    MediaBackendResponse.Done
                }
                is MediaBackendCommand.SetSurfaces -> {
                    session(runtime, command.sessionId)
                    runtime.compositor.set(command.sessionId.key(), command.viewportRevision, command.layoutRevision, command.surfaces)
                    MediaBackendResponse.Done
                }
                MediaBackendCommand.CloseRuntime -> { release(runtime); MediaBackendResponse.Done }
            }
        } catch (failure: MediaDomainFailure) {
            MediaBackendResponse.Rejected(MediaOperationFailure.Domain(failure.domain))
        } catch (failure: SecurityException) {
            MediaBackendResponse.Rejected(MediaOperationFailure.Denied)
        } catch (cancelled: CancellationException) {
            MediaBackendResponse.Rejected(MediaOperationFailure.Domain(HostMediaError.OperationCancelled))
        } catch (failure: NativeMediaException) {
            throw failure
        } catch (failure: Throwable) {
            release(runtime)
            throw NativeMediaException.BackendFailure()
        }
    }

    private suspend fun consent(runtime: Runtime, command: MediaBackendCommand.RequestConsent): MediaBackendResponse = coroutineScope {
        val key = command.operationId.key()
        if (runtime.closed || key in runtime.cancelled) throw MediaDomainFailure(HostMediaError.OperationCancelled)
        runtime.consentJobs[key] = currentCoroutineContext().job
        try {
            val granted = when (val request = command.request) {
                is MediaConsentRequest.Calling -> NativeMediaConsent.calling(context, productId, request.network, request.account)
                MediaConsentRequest.Microphone -> NativeMediaConsent.device(context, productId, Manifest.permission.RECORD_AUDIO)
                MediaConsentRequest.Camera -> NativeMediaConsent.device(context, productId, Manifest.permission.CAMERA)
            }
            if (runtime.closed || key in runtime.cancelled) throw MediaDomainFailure(HostMediaError.OperationCancelled)
            MediaBackendResponse.Consent(granted)
        } finally { runtime.consentJobs.remove(key) }
    }

    private suspend fun prepare(runtime: Runtime, session: Session, operationId: ByteArray, revision: ULong, tracks: MediaLocalTracks): MediaBackendResponse = coroutineScope {
        val key = operationId.key()
        if (key in runtime.cancelled || runtime.closed) throw MediaDomainFailure(HostMediaError.OperationCancelled)
        runtime.operations.filterValues { it.sessionKey == session.key }.keys.toList().forEach { cancel(runtime, it) }
        session.generation++
        val capture = NativeMediaCapture(context, core)
        val pending = Pending(session.key, session.generation, revision, tracks, capture, currentCoroutineContext().job)
        pending.microphoneAuthorized = tracks.microphone &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        pending.cameraAuthorized = tracks.camera &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        runtime.operations[key] = pending
        try {
            if (!NativeMediaService.indicatorAvailable(context)) throw SecurityException()
            if (tracks.microphone) {
                NativeMediaConsent.operatingSystemPermission(context, Manifest.permission.RECORD_AUDIO)
                pending.microphoneAuthorized = true
            }
            if (tracks.camera) {
                NativeMediaConsent.operatingSystemPermission(context, Manifest.permission.CAMERA)
                pending.cameraAuthorized = true
            }
            suspend fun foreground(projection: Boolean) {
                var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
                if (tracks.microphone || session.capture?.microphone != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                if (tracks.camera || session.capture?.camera != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA
                if (projection || session.capture?.screen != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
                pending.foregroundTypes = types
                MediaServiceLeases.acquire(context, session.lease, MediaServiceLeases.Lease(productId, types,
                    { scope.launch { if (runtime.sessions[session.key] === session) { end(session); emit(runtime, MediaBackendEvent.HostEnded(session.id)) } } },
                    { scope.launch { stopScreen(session) } }))
            }
            foreground(false)
            capture.reuse(session.capture, tracks)
            if (tracks.microphone && capture.microphone == null) capture.microphone()
            if (tracks.camera && capture.camera == null) capture.camera(tracks.cameraPreference) { scope.launch { session.capture?.let { localChanged(session, it) } } }
            if (tracks.screen && capture.screen == null) capture.screen({ stoppedTrack ->
                scope.launch {
                    if (session.capture?.screen != null && session.capture?.screen === stoppedTrack) stopScreen(session)
                    else if (runtime.operations[key] === pending) cancel(runtime, key)
                }
            }, { foreground(true) }, { scope.launch { session.capture?.let { localChanged(session, it) } } })
            if (runtime.closed || runtime.operations[key] !== pending || session.generation != pending.generation) throw MediaDomainFailure(HostMediaError.OperationCancelled)
            pending.ready = true
            MediaBackendResponse.Done
        } catch (error: Throwable) {
            runtime.operations.remove(key, pending)
            capture.close()
            if (!session.committed && runtime.sessions[session.key] === session) end(session)
            else if (session.committed) refreshLease(session)
            throw error
        }
    }

    private fun commit(runtime: Runtime, operationId: ByteArray): MediaBackendResponse {
        checkPermissions()
        val key = operationId.key()
        val pending = runtime.operations[key] ?: throw MediaDomainFailure(HostMediaError.OperationCancelled)
        val session = runtime.sessions[pending.sessionKey] ?: throw MediaDomainFailure(HostMediaError.SessionEnded)
        if (!pending.ready || session.generation != pending.generation) throw MediaDomainFailure(HostMediaError.InvalidState)
        val old = session.capture
        // No coroutine suspension in this commit. New tracks remain disabled until every sender switched.
        old?.disable()
        try {
            session.peers.values.forEach { it.setTracks(pending.capture.tracks()) }
            runtime.audio.select(pending.tracks.audioPreference)
        } catch (failure: Throwable) {
            val restored = runCatching {
                session.peers.values.forEach { it.setTracks(old?.tracks() ?: listOf(null, null, null)) }
                old?.enable()
            }
            pending.capture.close()
            runtime.operations.remove(key)
            if (!session.committed) end(session) else refreshLease(session)
            check(restored.isSuccess) { "Media sender restore failed" }
            throw failure
        }
        session.capture = pending.capture
        session.tracks = pending.tracks
        session.revision = pending.revision
        session.committed = true
        session.screenStopped = false
        runtime.operations.remove(key)
        try {
            pending.capture.enable()
            runtime.compositor.refresh()
        } finally { old?.close() }
        refreshLease(session)
        session.peers.values.toList().forEach { it.renegotiateLater() }
        return MediaBackendResponse.LocalState(pending.capture.state(runtime.audio.current()))
    }

    private fun cancel(runtime: Runtime, key: String) {
        runtime.cancelled.add(key)
        runtime.consentJobs.remove(key)?.cancel()
        val pending = runtime.operations.remove(key) ?: return
        pending.job.cancel()
        pending.capture.close()
        val session = runtime.sessions[pending.sessionKey] ?: return
        if (!session.committed) end(session) else refreshLease(session)
    }

    private fun stopScreen(session: Session) {
        if (session.screenStopped) return
        session.screenStopped = true
        session.runtime.operations.filterValues { it.sessionKey == session.key && it.tracks.screen }.keys.toList().forEach { cancel(session.runtime, it) }
        val capture = session.capture
        if (capture?.screen != null) {
            session.peers.values.forEach { it.setTracks(listOf(capture.microphone, capture.camera, null)) }
            capture.screen?.setEnabled(false)
            session.runtime.compositor.refresh()
            capture.stopScreen()
            session.peers.values.toList().forEach { it.renegotiateLater() }
        }
        refreshLease(session)
        emit(session.runtime, MediaBackendEvent.ScreenStopped(session.id))
    }

    private fun refreshLease(session: Session) {
        var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
        if (session.capture?.microphone != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        if (session.capture?.camera != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA
        if (session.capture?.screen != null) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
        session.runtime.operations.values.forEach {
            if (it.sessionKey == session.key) types = types or it.foregroundTypes
        }
        MediaServiceLeases.update(context, session.lease, types)
    }

    private fun localChanged(session: Session, capture: NativeMediaCapture) {
        if (session.capture !== capture || session.runtime.closed) return
        checkPermissions()
        if (session.capture !== capture || session.runtime.sessions[session.key] !== session) return
        emit(session.runtime, MediaBackendEvent.LocalStateChanged(session.id, session.revision, capture.state(session.runtime.audio.current())))
        session.runtime.compositor.refresh()
    }

    private fun audioChanged() { scope.launch { runtimes.values.forEach { runtime -> runtime.sessions.values.forEach { session -> session.capture?.let { localChanged(session, it) } } } } }
    private fun checkPermissions() {
        for ((permission, revoked) in listOf(Manifest.permission.CAMERA to MediaRevokedPermission.CAMERA, Manifest.permission.RECORD_AUDIO to MediaRevokedPermission.MICROPHONE)) {
            if (ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED) continue
            runtimes.values.toList().forEach { runtime ->
                fun MediaLocalTracks.requiresPermission() =
                    if (revoked == MediaRevokedPermission.CAMERA) camera else microphone
                fun NativeMediaCapture?.usesPermission() =
                    if (revoked == MediaRevokedPermission.CAMERA) this?.camera != null else this?.microphone != null
                val sessions = runtime.sessions.values.filter {
                    it.committed && (it.tracks.requiresPermission() || it.capture.usesPermission())
                }
                val operations = runtime.operations.filterValues {
                    val authorized = if (revoked == MediaRevokedPermission.CAMERA) it.cameraAuthorized else it.microphoneAuthorized
                    (it.tracks.requiresPermission() && authorized) || it.capture.usesPermission()
                }.keys.toList()
                if (sessions.isNotEmpty() || operations.isNotEmpty()) {
                    operations.forEach { cancel(runtime, it) }
                    sessions.forEach(::end)
                    emit(runtime, MediaBackendEvent.PermissionRevoked(revoked, MediaRevocationSource.OPERATING_SYSTEM))
                }
            }
        }
    }

    fun attach(webView: WebView) {
        check(Looper.myLooper() == Looper.getMainLooper())
        view = webView
        runtimes.values.forEach { it.compositor.attach(webView) }
    }

    fun viewportChanged() { runtimes.values.forEach { it.compositor.invalidateViewport() } }

    private fun end(session: Session) {
        val runtime = session.runtime
        if (runtime.sessions.remove(session.key) !== session) return
        session.generation++
        runtime.operations.filterValues { it.sessionKey == session.key }.keys.toList().forEach { runCatching { cancel(runtime, it) } }
        runCatching { runtime.compositor.remove(session.key) }
        session.peers.values.toList().forEach { runCatching { it.close() } }
        session.peers.clear()
        runCatching { session.capture?.close() }
        session.capture = null
        runCatching { MediaServiceLeases.release(context, session.lease) }
        if (runtime.sessions.isEmpty()) runCatching { runtime.audio.close() }
    }

    private fun release(runtime: Runtime) {
        if (runtime.closed) return
        runtime.closed = true
        closedRuntimes.add(runtime.id)
        runtime.consentJobs.values.toList().forEach { it.cancel() }
        runtime.consentJobs.clear()
        runtime.sessions.values.toList().forEach(::end)
        runCatching { runtime.compositor.close() }
        runCatching { runtime.audio.close() }
        runtime.sink = null
        runtimes.remove(runtime.id)
    }

    private fun emit(runtime: Runtime, event: MediaBackendEvent) {
        if (runtime.closed) return
        try { runtime.sink?.publish(event) } catch (_: Exception) { release(runtime) }
    }
    private fun owner(product: ProductContext) { if (product.productId.value != productId) throw NativeMediaException.BackendFailure() }
    private fun session(runtime: Runtime, id: ByteArray): Session = runtime.sessions[id.key()] ?: throw MediaDomainFailure(HostMediaError.InvalidHandle)
    private fun peer(runtime: Runtime, id: ByteArray, participant: ByteArray): NativeMediaPeer = session(runtime, id).peers[participant.key()] ?: throw MediaDomainFailure(HostMediaError.InvalidHandle)

    override fun close() {
        scope.launch {
            if (closed) return@launch
            closed = true
            runtimes.values.toList().forEach(::release)
            appOps.stopWatchingMode(permissionsListener)
            context.getSystemService(AudioManager::class.java).unregisterAudioDeviceCallback(audioListener)
            view = null
            scope.cancel()
        }
    }
}

internal fun ByteArray.key(): String = joinToString("") { "%02x".format(it) }

/** Optional credentials must leave room within the core's five-second CreatePeer deadline. */
internal suspend fun nativeMediaIceServers(
    getConfig: suspend (Duration) -> ExternalRtcConfig,
): List<PeerConnection.IceServer> {
    currentCoroutineContext().ensureActive()
    return try {
        val config = getConfig(2.seconds)
        currentCoroutineContext().ensureActive()
        config.toIceServers()
    } catch (failure: Exception) {
        // Provider-local cancellation is a service failure, not cancellation of this command.
        currentCoroutineContext().ensureActive()
        emptyList()
    }
}
