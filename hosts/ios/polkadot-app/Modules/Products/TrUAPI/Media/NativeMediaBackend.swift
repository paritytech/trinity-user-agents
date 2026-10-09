import Foundation
import UIKit
import WebKit
import AVFoundation
import WebRTC
import TrUAPIHost
import SubstrateSdk
import StructuredConcurrency

/// One instance is retained by one Rust product execution (App or Worker).
/// Rust supplies and authenticates every session/participant and signal.
final class NativeMediaBackend: NativeMediaCallbacks, @unchecked Sendable {
    static let permissionRevokedNotification = Notification.Name("HostMediaProductPermissionRevoked")
    let productId: String
    private let lock = NSLock()
    private var runtimeId: UInt64?
    private var sink: NativeMediaEventSink?
    private var closed = false
    @MainActor private var currentRuntime: NativeMediaRuntime?
    @MainActor private var runtime: NativeMediaRuntime {
        if let currentRuntime { return currentRuntime }
        let runtime = NativeMediaRuntime(productId: productId) { [weak self] in self?.publish($0) }
        currentRuntime = runtime
        return runtime
    }
    @MainActor private var engineObservation: Task<Void, Never>?

    init(productId: String) { self.productId = productId }

    private var isClosed: Bool {
        lock.lock(); defer { lock.unlock() }
        return closed
    }

    @MainActor private func perform(_ command: NativeMediaBackendCommand) async throws -> NativeMediaBackendResponse {
        guard !isClosed else { throw NativeMediaError.Closed }
        return try await runtime.command(command)
    }

    private func bind(product: ProductContext, runtimeId: UInt64, allowClosed: Bool = false) throws {
        lock.lock(); defer { lock.unlock() }
        guard (!closed || allowClosed), product.productId == productId,
              product.executionKind == .app || product.executionKind == .worker,
              self.runtimeId == nil || self.runtimeId == runtimeId else { throw NativeMediaError.Closed }
        self.runtimeId = runtimeId
    }

    static func devicePermissionStatus(_ request: HostDevicePermissionRequest) -> DevicePermissionStatus {
        let status: AVAuthorizationStatus
        switch request {
        case .camera: status = AVCaptureDevice.authorizationStatus(for: .video)
        case .microphone: status = AVCaptureDevice.authorizationStatus(for: .audio)
        default: return .notApplicable
        }
        switch status {
        case .authorized: return .granted
        case .notDetermined: return .notDetermined
        case .denied, .restricted: return .denied
        @unknown default: return .denied
        }
    }

    func capabilities(product: ProductContext) async throws -> NativeMediaBackendCapabilities {
        guard !isClosed, product.productId == productId else { throw NativeMediaError.Closed }
        let supported = await NativeMediaPresentation.available
        return NativeMediaBackendCapabilities(supported: supported, maxSessions: supported ? 1 : 0,
            maxRemoteParticipants: supported ? 5 : 0, maxSurfacesPerSession: supported ? 12 : 0)
    }

    func subscribe(product: ProductContext, runtimeId: UInt64, eventSink: NativeMediaEventSink) throws {
        try bind(product: product, runtimeId: runtimeId)
        lock.lock()
        guard !closed else { lock.unlock(); throw NativeMediaError.Closed }
        sink = eventSink
        lock.unlock()
        Task { @MainActor [weak self] in
            guard let self, !self.isClosed else { return }
            self.publish(.viewportChanged(viewport: self.runtime.compositor.viewport))
        }
    }

    func unsubscribe(runtimeId: UInt64) {
        lock.lock()
        let matches = self.runtimeId == runtimeId
        if matches { closed = true; sink = nil }
        lock.unlock()
        if matches { Task { await self.close() } }
    }

    func command(product: ProductContext, runtimeId: UInt64, command: NativeMediaBackendCommand) async throws -> NativeMediaBackendResponse {
        if case .closeRuntime = command {
            try bind(product: product, runtimeId: runtimeId, allowClosed: true)
            await close()
            return .done
        }
        try bind(product: product, runtimeId: runtimeId)
        do {
            return try await withTaskCancellationHandler {
                try await perform(command)
            } onCancel: {
                Task { @MainActor [weak self] in self?.currentRuntime?.cancelCommand(command) }
            }
        } catch let failure as NativeMediaFailure { return failure.response }
        catch is CancellationError { return NativeMediaFailure.cancelled.response }
        catch { throw NativeMediaError.BackendFailure }
    }

    private func publish(_ event: NativeMediaBackendEvent) {
        lock.lock(); let sink = sink; let closed = closed; lock.unlock()
        guard !closed, let sink else { return }
        do { try sink.publish(event: event) }
        catch { Task { await self.close() } }
    }

    @MainActor func attach(_ webView: WKWebView) {
        guard !isClosed else { return }
        runtime.compositor.attach(webView)
    }
    @MainActor func detach() { currentRuntime?.compositor.detach() }

    @MainActor func watchEngine(isAlive: @escaping @Sendable () async -> Bool) {
        guard !isClosed else { return }
        engineObservation?.cancel()
        engineObservation = Task { [weak self] in
            while !Task.isCancelled {
                guard await isAlive() else { await self?.close(); return }
                do { try await Task.sleep(for: .milliseconds(500)) } catch { return }
            }
        }
    }

    @MainActor private func stopEngineObservation() {
        engineObservation?.cancel()
        engineObservation = nil
    }

    func close() async {
        markClosed()
        await stopEngineObservation()
        await closeRuntimeIfPresent()
    }

    @MainActor private func closeRuntimeIfPresent() async { await currentRuntime?.close() }

    private func markClosed() {
        lock.lock(); closed = true; sink = nil; lock.unlock()
    }

    func authorityLost() {
        Task { @MainActor [weak self] in self?.currentRuntime?.authorityLost() }
    }
}

@MainActor
private final class NativeMediaRuntime {
    @MainActor private final class Pending {
        let sessionId: Data
        let revision: UInt64
        var cancelled = false
        var capture: NativeMediaCapture?
        var preparing = false
        init(sessionId: Data = Data(), revision: UInt64 = 0) { self.sessionId = sessionId; self.revision = revision }
    }

    private static var captureOwner: UUID?
    private let owner = UUID()
    private let productId: String
    private let emit: (NativeMediaBackendEvent) -> Void
    private let audioDevice = NativeMediaAudioDevice()
    private lazy var factory = WebRTCPeerConnectionFactoryProvider.make(audioDevice: audioDevice)
    private let configuration = WebRTCConfigFactory(turnService: TURNCredentialsService(
        requestFactory: TURNCredentialsRequestFactory(), tokenProvider: JWTTokenManager.shared))
    private let presentation: NativeMediaPresentation
    let compositor = NativeMediaCompositor()
    private var sessionId: Data?
    private var capture: NativeMediaCapture?
    private var intentRevision: UInt64 = 0
    private var pending: [Data: Pending] = [:]
    private var cancelled: Set<Data> = []
    private var promptOperation: Data?
    private var peers: [Data: NativeMediaPeer] = [:]
    private var peerGenerations: [Data: UUID] = [:]
    private var remotePictures: [Data: [RTCVideoTrack?]] = [:]
    private var surfaces: [NativeMediaSurface] = []
    private var observation: Task<Void, Never>?
    private var notifications: [NSObjectProtocol] = []
    private var lastState: NativeMediaLocalState?
    private var audioInterrupted = false
    private var committing = false
    private let senderMutations = SerialOperationQueue()
    private var closed = false

    init(productId: String, emit: @escaping (NativeMediaBackendEvent) -> Void) {
        self.productId = productId; self.emit = emit
        presentation = NativeMediaPresentation(productId: productId)
        compositor.onViewport = { [weak self] viewport in
            self?.surfaces = []
            self?.emit(.viewportChanged(viewport: viewport))
        }
        presentation.onEnd = { [weak self] in
            guard let self else { return }
            let sessionId = self.sessionId
            self.authorityLost()
            if let sessionId { self.emit(.hostEnded(sessionId: sessionId)) }
        }
        presentation.onMute = { [weak self] muted in
            self?.audioDevice.setMuted(muted)
            self?.publishLocal()
        }
        presentation.onScreenStop = { [weak self] in self?.screenStopped() }
        presentation.onAudioActivation = { [weak self] active in
            guard let self else { return }
            self.audioInterrupted = !active
            CallAudioSessionManager.shared.activateMedia(owner: self.owner, active: active)
            self.audioDevice.setActive(active)
            self.publishLocal()
        }
        notifications.append(NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification,
            object: nil, queue: .main) { [weak self] notification in
                let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt
                let options = AVAudioSession.InterruptionOptions(
                    rawValue: notification.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0)
                MainActor.assumeIsolated {
                    guard let self else { return }
                    let mayResume = raw == AVAudioSession.InterruptionType.ended.rawValue && options.contains(.shouldResume)
                    self.audioInterrupted = !mayResume
                    self.audioDevice.setActive(!self.audioInterrupted)
                    self.publishLocal()
                }
            })
        notifications.append(NotificationCenter.default.addObserver(forName: NativeMediaBackend.permissionRevokedNotification,
            object: nil, queue: .main) { [weak self] notification in
                guard notification.userInfo?["productId"] as? String == productId,
                      let permission = notification.userInfo?["permission"] as? NativeMediaRevokedPermission else { return }
                MainActor.assumeIsolated {
                    guard let self, !self.closed else { return }
                    func requires(_ tracks: NativeMediaLocalTracks) -> Bool {
                        switch permission {
                        case .calling: return true
                        case .microphone: return tracks.microphone
                        case .camera: return tracks.camera
                        }
                    }
                    if permission == .calling || self.capture.map({ requires($0.intent) }) == true {
                        self.authorityLost()
                    } else {
                        // Cancel only dependent preparation; withdrawing an
                        // unused device must not destroy a receive-only call.
                        for (id, operation) in self.pending {
                            if let prepared = operation.capture, requires(prepared.intent) { self.cancel(id) }
                        }
                    }
                    self.emit(.permissionRevoked(permission: permission, source: .product))
                }
            })
    }

    func command(_ command: NativeMediaBackendCommand) async throws -> NativeMediaBackendResponse {
        guard !closed else { throw NativeMediaError.Closed }
        switch command {
        case let .requestConsent(operationId, request):
            return try await consent(operationId: operationId, request: request)
        case let .openSession(sessionId, operationId, tracks):
            return try await queuePrepare(sessionId: sessionId, operationId: operationId,
                revision: 0, tracks: tracks, opening: true)
        case let .setTracks(sessionId, operationId, revision, tracks):
            return try await queuePrepare(sessionId: sessionId, operationId: operationId,
                revision: revision, tracks: tracks, opening: false)
        case let .commitOperation(operationId):
            return try await senderMutations.run { try await self.commit(operationId) }
        case let .cancelOperation(operationId):
            cancel(operationId); return .done
        case let .closeSession(sessionId):
            if self.sessionId == sessionId { await closeSession() }
            else { for (id, operation) in pending where operation.sessionId == sessionId { cancel(id) } }
            return .done
        case let .createPeer(sessionId, participantId, offerer):
            try requireSession(sessionId)
            let rtcConfiguration = try await configuration.makeConnectionConfiguration()
            return try await senderMutations.run {
                try await self.createPeer(sessionId: sessionId, participantId: participantId,
                    offerer: offerer, configuration: rtcConfiguration)
            }
        case let .applyDescription(sessionId, participantId, description):
            try requireSession(sessionId)
            guard let peer = peers[participantId] else { throw NativeMediaFailure.domain(.invalidState) }
            try await peer.apply(description); return .done
        case let .addIceCandidate(sessionId, participantId, candidate):
            try requireSession(sessionId)
            guard let peer = peers[participantId] else { throw NativeMediaFailure.domain(.invalidState) }
            try await peer.add(candidate); return .done
        case let .removePeer(sessionId, participantId):
            if self.sessionId == sessionId {
                let peer = peers.removeValue(forKey: participantId)
                peerGenerations.removeValue(forKey: participantId)
                remotePictures.removeValue(forKey: participantId)
                surfaces.removeAll { if case let .remote(id, _) = $0.source { return id == participantId }; return false }
                compositor.removeParticipant(participantId)
                refreshPictures(); await peer?.close()
            }
            return .done
        case let .setSurfaces(sessionId, viewportRevision, layoutRevision, surfaces):
            try requireSession(sessionId)
            try compositor.set(viewportRevision: viewportRevision, layoutRevision: layoutRevision, surfaces: surfaces, track: picture)
            self.surfaces = surfaces
            return .done
        case .closeRuntime:
            await close(); return .done
        }
    }

    private func createPeer(sessionId: Data, participantId: Data, offerer: Bool,
                            configuration: RTCConfiguration) async throws -> NativeMediaBackendResponse {
        try requireSession(sessionId)
        guard capture != nil, peers[participantId] == nil, peers.count < 5 else {
            throw NativeMediaFailure.domain(.invalidState)
        }
        guard let connection = factory.peerConnection(with: configuration,
            constraints: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil), delegate: nil) else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        let generation = UUID()
        peerGenerations[participantId] = generation
        let peer = NativeMediaPeer(connection: connection, sessionId: sessionId, participantId: participantId,
            offerer: offerer, emit: { [weak self] event in
                Task { @MainActor in
                    guard let self, self.sessionId == sessionId, self.peerGenerations[participantId] == generation else { return }
                    self.emit(event)
                }
            }, pictures: { [weak self] tracks in
                Task { @MainActor in
                    guard let self, self.sessionId == sessionId, self.peerGenerations[participantId] == generation else { return }
                    self.remotePictures[participantId] = tracks
                    self.refreshPictures()
                }
            })
        peers[participantId] = peer
        do {
            try await peer.start(tracks: capture?.tracks ?? [nil, nil, nil], offerer: offerer)
            try requireSession(sessionId)
            guard peerGenerations[participantId] == generation else { throw NativeMediaFailure.cancelled }
        } catch {
            if peerGenerations[participantId] == generation {
                peerGenerations.removeValue(forKey: participantId)
                peers.removeValue(forKey: participantId)
            }
            await peer.close()
            throw error
        }
        return .done
    }

    private func requireSession(_ id: Data) throws {
        guard !closed, sessionId == id else { throw NativeMediaFailure.domain(.invalidState) }
    }
}

// MARK: - Consent and capture operations

extension NativeMediaRuntime {

    private func consent(operationId: Data, request: NativeMediaConsentRequest) async throws -> NativeMediaBackendResponse {
        guard !cancelled.contains(operationId), promptOperation == nil else { throw NativeMediaFailure.cancelled }
        if pending[operationId] == nil { pending[operationId] = Pending() }
        promptOperation = operationId
        defer { if promptOperation == operationId { promptOperation = nil } }
        let granted: Bool
        switch request {
        case let .calling(network, account):
            granted = try await presentation.confirmCalling(network: network, account: account)
        case .microphone:
            granted = try await presentation.confirm(title: "Allow microphone?",
                detail: "Allow \(productId) to use the microphone through host-owned Media?")
        case .camera:
            granted = try await presentation.confirm(title: "Allow camera?",
                detail: "Allow \(productId) to use the camera through host-owned Media?")
        }
        guard !closed, !cancelled.contains(operationId), !Task.isCancelled else { throw NativeMediaFailure.cancelled }
        return .consent(granted: granted)
    }

    private func queuePrepare(sessionId: Data, operationId: Data, revision: UInt64,
                              tracks: NativeMediaLocalTracks, opening: Bool) async throws -> NativeMediaBackendResponse {
        guard !cancelled.contains(operationId) else { throw NativeMediaFailure.cancelled }
        // Register before enqueueing so Cancel/Close can fence queued work too.
        let operation = Pending(sessionId: sessionId, revision: revision)
        operation.preparing = true
        pending[operationId] = operation
        do {
            return try await senderMutations.run { @MainActor in
                guard !self.closed, !operation.cancelled, self.pending[operationId] === operation else {
                    throw NativeMediaFailure.cancelled
                }
                if opening {
                    guard self.sessionId == nil, Self.captureOwner == nil || Self.captureOwner == self.owner else {
                        throw NativeMediaFailure.domain(.deviceUnavailable)
                    }
                    Self.captureOwner = self.owner
                    self.sessionId = sessionId
                } else {
                    try self.requireSession(sessionId)
                    guard revision > self.intentRevision else { throw NativeMediaFailure.domain(.invalidState) }
                }
                try await self.prepare(operation: operation, operationId: operationId, tracks: tracks)
                return .done
            }
        } catch {
            if pending[operationId] === operation { pending.removeValue(forKey: operationId) }
            if opening, capture == nil, self.sessionId == sessionId { await closeSession() }
            throw error
        }
    }

    private func prepare(operation: Pending, operationId: Data, tracks: NativeMediaLocalTracks) async throws {
        try check(operation, operationId)
        let prepared = NativeMediaCapture(intent: tracks, audio: audioDevice)
        operation.capture = prepared; pending[operationId] = operation
        try presentation.show(pending: true, state: capture?.state(interrupted: audioInterrupted))
        do {
            if tracks.microphone {
                if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
                    _ = await AVCaptureDevice.requestAccess(for: .audio)
                    try check(operation, operationId)
                }
                guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else { throw NativeMediaFailure.denied }
                if let existing = capture?.microphone { prepared.microphone = existing }
                else {
                    let source = factory.audioSource(with: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil))
                    prepared.microphone = factory.audioTrack(with: source, trackId: UUID().uuidString)
                    prepared.microphone?.isEnabled = false
                }
            }
            if tracks.camera {
                if AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined {
                    _ = await AVCaptureDevice.requestAccess(for: .video)
                    try check(operation, operationId)
                }
                guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized else { throw NativeMediaFailure.denied }
                if let existing = capture?.camera { prepared.camera = existing }
                else {
                    let camera = NativeMediaCamera(factory: factory); prepared.camera = camera
                    try await camera.start(preference: tracks.cameraPreference)
                }
            }
            try check(operation, operationId)
            if tracks.screen {
                if let existing = capture?.screen { prepared.screen = existing }
                else {
                    let screen = try NativeMediaScreen(factory: factory); prepared.screen = screen
                    screen.onStop = { [weak self] in self?.screenStopped() }
                    try await screen.start(presentation: presentation)
                }
            }
            try check(operation, operationId)
            operation.preparing = false
        } catch {
            prepared.stop(except: capture)
            pending.removeValue(forKey: operationId)
            if capture != nil { try? presentation.show(pending: false, state: capture?.state(interrupted: audioInterrupted)) }
            throw error
        }
    }

    private func check(_ operation: Pending, _ id: Data) throws {
        guard !closed, !operation.cancelled, !Task.isCancelled,
              pending[id] === operation, sessionId == operation.sessionId else { throw NativeMediaFailure.cancelled }
    }

    private func commit(_ operationId: Data) async throws -> NativeMediaBackendResponse {
        guard let operation = pending[operationId], let prepared = operation.capture,
              !operation.preparing, !committing else { throw NativeMediaFailure.domain(.invalidState) }
        try check(operation, operationId)
        committing = true
        defer { committing = false }
        let old = capture
        do {
            audioDevice.setActive(false)
            try CallAudioSessionManager.shared.acquireMedia(owner: owner, microphone: prepared.microphone != nil,
                speaker: prepared.intent.audioPreference == .speaker || prepared.intent.camera)
            _ = factory
            try audioDevice.configure(microphone: prepared.microphone != nil, active: false)
            try await presentation.beginSystemCall()
            try check(operation, operationId)
            // Disable every source while the serialized sender replacements are
            // installed. No newly prepared source sends before the whole set wins.
            old?.setEnabled(false); prepared.setEnabled(false)
            for peer in peers.values { try await peer.replace(tracks: prepared.tracks); try check(operation, operationId) }
            try audioDevice.configure(microphone: prepared.microphone != nil, active: !audioInterrupted)
            try presentation.show(pending: false, state: prepared.state(interrupted: audioInterrupted))
            capture = prepared; intentRevision = operation.revision
            pending.removeValue(forKey: operationId)
            old?.stop(except: prepared)
            prepared.setEnabled(true)
            if let preference = prepared.intent.audioPreference {
                switch preference {
                case .speaker: try? CallAudioSessionManager.shared.selectRoute(.builtInSpeaker)
                case .earpiece: try? CallAudioSessionManager.shared.selectRoute(.builtInReceiver)
                case .headset:
                    if let route = CallAudioSessionManager.shared.routeState.availableRoutes.first(where: {
                        if case .external = $0 { return true }; return false
                    }) { try? CallAudioSessionManager.shared.selectRoute(route) }
                case .other: break
                }
            }
            let state = prepared.state(interrupted: audioInterrupted)
            startObservation(); refreshPictures()
            for peer in peers.values {
                Task { [weak self] in
                    do { try await peer.renegotiate() }
                    catch {
                        guard let self, self.sessionId == operation.sessionId else { return }
                        self.emit(.peerStateChanged(sessionId: operation.sessionId,
                            participantId: peer.participantId, state: .failed))
                        await peer.close()
                    }
                }
            }
            return .localState(state: state)
        } catch {
            if sessionId == operation.sessionId, !closed {
                for peer in peers.values { try? await peer.replace(tracks: old?.tracks ?? [nil, nil, nil]) }
                prepared.stop(except: old); old?.setEnabled(true)
                capture = old
                if let old {
                    try? CallAudioSessionManager.shared.acquireMedia(owner: owner, microphone: old.microphone != nil,
                        speaker: old.intent.audioPreference == .speaker || old.intent.camera)
                    try? audioDevice.configure(microphone: old.microphone != nil, active: !audioInterrupted)
                }
                if old == nil { await closeSession() }
            } else { prepared.stop() }
            pending.removeValue(forKey: operationId)
            throw error
        }
    }

    func cancelCommand(_ command: NativeMediaBackendCommand) {
        switch command {
        case let .openSession(_, operationId, _), let .setTracks(_, operationId, _, _), let .requestConsent(operationId, _):
            cancel(operationId)
        default: break
        }
    }

    private func cancel(_ id: Data) {
        cancelled.insert(id)
        let operation = pending.removeValue(forKey: id)
        operation?.cancelled = true
        operation?.capture?.stop(except: capture)
        if promptOperation == id { presentation.cancelPrompt(); promptOperation = nil }
        if operation?.capture?.intent.screen == true { presentation.dismissPicker() }
        if capture == nil, sessionId == nil || operation?.sessionId == sessionId {
            presentation.close(); sessionId = nil
            audioDevice.stop()
            CallAudioSessionManager.shared.releaseMedia(owner: owner)
            if Self.captureOwner == owner { Self.captureOwner = nil }
        }
    }
}

// MARK: - Session observation and lifecycle

extension NativeMediaRuntime {

    private func picture(_ source: NativeMediaPictureSource) throws -> RTCVideoTrack? {
        switch source {
        case let .local(kind):
            let video = kind == .camera ? capture?.camera?.video : capture?.screen?.video
            return video?.state == .live ? video?.track : nil
        case let .remote(participantId, kind):
            guard peers[participantId] != nil else { throw NativeMediaFailure.domain(.invalidSurface) }
            return remotePictures[participantId]?[kind == .camera ? 0 : 1]
        }
    }

    private func refreshPictures() {
        guard !surfaces.isEmpty else { compositor.clear(); return }
        // Internal refresh does not consume a public layout revision.
        compositor.refresh(track: picture)
    }

    private func screenStopped() {
        for (id, operation) in pending where operation.capture?.intent.screen == true {
            operation.capture?.screen?.stop()
            operation.capture?.screen = nil
            if !committing { cancel(id) }
        }
        presentation.dismissPicker()
        guard let sessionId else { return }
        capture?.screen?.stop(); capture?.screen = nil
        refreshPictures()
        Task { [weak self] in
            guard let self else { return }
            await self.senderMutations.run { @MainActor in
                guard !self.closed, self.sessionId == sessionId else { return }
                let tracks = self.capture?.tracks ?? [nil, nil, nil]
                for peer in self.peers.values {
                    do { try await peer.replace(tracks: tracks); try await peer.renegotiate() }
                    catch {
                        guard self.sessionId == sessionId else { return }
                        self.emit(.peerStateChanged(sessionId: sessionId,
                            participantId: peer.participantId, state: .failed))
                        await peer.close()
                    }
                }
            }
        }
        emit(.screenStopped(sessionId: sessionId))
        publishLocal()
    }

    private func startObservation() {
        guard observation == nil else { return }
        observation = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, !self.closed, self.sessionId != nil else { return }
                if self.capture?.microphone != nil && AVCaptureDevice.authorizationStatus(for: .audio) != .authorized {
                    self.emit(.permissionRevoked(permission: .microphone, source: .operatingSystem)); await self.closeSession(); return
                }
                if self.capture?.camera != nil && AVCaptureDevice.authorizationStatus(for: .video) != .authorized {
                    self.emit(.permissionRevoked(permission: .camera, source: .operatingSystem)); await self.closeSession(); return
                }
                self.publishLocal()
                do { try await Task.sleep(for: .milliseconds(500)) } catch { return }
            }
        }
    }

    private func publishLocal() {
        guard let sessionId, let capture, !closed else { return }
        let state = capture.state(interrupted: audioInterrupted)
        guard state != lastState else { return }
        lastState = state
        refreshPictures()
        try? presentation.show(pending: false, state: state)
        emit(.localStateChanged(sessionId: sessionId, intentRevision: intentRevision, state: state))
    }

    private func stopSession() -> [Data: NativeMediaPeer] {
        audioDevice.stop()
        audioDevice.setMuted(false)
        let peers = peers
        self.peers.removeAll(); sessionId = nil
        peerGenerations.removeAll()
        for operation in pending.values { operation.cancelled = true; operation.capture?.stop(except: capture) }
        pending.removeAll(); promptOperation = nil
        capture?.stop(); capture = nil
        surfaces.removeAll(); remotePictures.removeAll(); compositor.resetSession()
        presentation.close()
        observation?.cancel(); observation = nil; lastState = nil
        CallAudioSessionManager.shared.releaseMedia(owner: owner)
        if Self.captureOwner == owner { Self.captureOwner = nil }
        return peers
    }

    private func closeSession() async {
        let peers = stopSession()
        for peer in peers.values { await peer.close() }
    }

    func authorityLost() {
        let peers = stopSession()
        Task { for peer in peers.values { await peer.close() } }
    }

    func close() async {
        guard !closed else { return }
        closed = true
        await closeSession(); compositor.detach()
        for observer in notifications { NotificationCenter.default.removeObserver(observer) }
        notifications.removeAll()
    }
}
