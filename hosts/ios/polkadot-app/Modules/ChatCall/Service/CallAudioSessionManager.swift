import Foundation
import AVFoundation
import WebRTC
import AsyncExtensions

protocol CallAudioSessionManaging: AnyObject {
    var routeState: CallAudioRouteState { get }

    func configureAudioSession(for callType: ChatCallType) throws

    func setEnabled(_ isEnabled: Bool, for session: AVAudioSession)

    func enableAudioIfPermitted()

    func selectRoute(_ route: CallAudioRoute) throws

    func observeRouteState() -> AnyAsyncSequence<CallAudioRouteState>
}

final class CallAudioSessionManager {
    static let shared = CallAudioSessionManager()

    private let audioSession: RTCAudioSession
    private let systemAudioSession: AVAudioSession
    private let routeStateSubject: AsyncCurrentValueSubject<CallAudioRouteState>
    private let recordPermissionProvider: RecordPermissionProviding
    private let pendingSessionLock = NSLock()

    private var routeObservationTask: Task<Void, Never>?
    private var sessionPendingPermission: AVAudioSession?

    var routeState: CallAudioRouteState { routeStateSubject.value }

    init(
        audioSession: RTCAudioSession = .sharedInstance(),
        systemAudioSession: AVAudioSession = .sharedInstance(),
        recordPermissionProvider: RecordPermissionProviding = RecordPermissionService()
    ) {
        self.audioSession = audioSession
        self.systemAudioSession = systemAudioSession
        self.recordPermissionProvider = recordPermissionProvider
        audioSession.useManualAudio = true
        audioSession.isAudioEnabled = false

        routeStateSubject = AsyncCurrentValueSubject(.unknown)
    }

    deinit {
        stopRouteObservation()
    }
}

private extension CallAudioSessionManager {
    func setupAudioSessionConfig(for callType: ChatCallType) throws {
        let config = RTCAudioSessionConfiguration.webRTC()
        config.category = AVAudioSession.Category.playAndRecord.rawValue

        switch callType {
        case .audio:
            config.mode = AVAudioSession.Mode.voiceChat.rawValue
            config.categoryOptions = [
                .allowBluetoothHFP
            ]
        case .video:
            config.mode = AVAudioSession.Mode.videoChat.rawValue
            config.categoryOptions = [
                .defaultToSpeaker,
                .allowBluetoothHFP
            ]
        }

        try audioSession.setConfiguration(config)
    }

    func setupInitialLoudspeakerBehavior(for callType: ChatCallType) throws {
        // `currentRoute` alone isn't reliable here — before RTCAudioSession
        // activation it can still reflect pre-call routing. Check availableInputs
        // too so a connected-but-not-yet-routed BT/wired device suppresses the
        // speaker default.
        if hasExternalPortAvailable() {
            try audioSession.overrideOutputAudioPort(.none)
            return
        }

        switch callType {
        case .audio:
            try audioSession.overrideOutputAudioPort(.none)
        case .video:
            try audioSession.overrideOutputAudioPort(.speaker)
        }
    }

    func hasExternalPortAvailable() -> Bool {
        let externalTypes = CallAudioRouteState.externalOutputPorts
        let inInputs = (systemAudioSession.availableInputs ?? [])
            .contains { externalTypes.contains($0.portType) }
        let inOutputs = systemAudioSession.currentRoute.outputs
            .contains { externalTypes.contains($0.portType) }
        return inInputs || inOutputs
    }

    func startRouteObservation() {
        stopRouteObservation()

        let routeNotifications = NotificationCenter.default.notifications(
            named: AVAudioSession.routeChangeNotification
        )

        routeObservationTask = Task { [weak self] in
            for await _ in routeNotifications {
                guard let self else { return }
                updateRouteState()
            }
        }

        updateRouteState()
    }

    @discardableResult
    func replaceSessionPendingPermission(with session: AVAudioSession?) -> AVAudioSession? {
        pendingSessionLock.withLock {
            let previous = sessionPendingPermission
            sessionPendingPermission = session
            return previous
        }
    }

    func stopRouteObservation() {
        routeObservationTask?.cancel()
        routeObservationTask = nil
    }

    func computeState() -> CallAudioRouteState {
        var externalRoutes: [CallAudioRoute] = []
        var seenKeys = Set<String>()

        let candidates = systemAudioSession.currentRoute.outputs
            + (systemAudioSession.availableInputs ?? [])

        for port in candidates {
            guard CallAudioRouteState.externalOutputPorts.contains(port.portType) else {
                continue
            }
            guard seenKeys.insert(port.callDeviceIdentifier).inserted else {
                continue
            }
            externalRoutes.append(.external(port))
        }

        var routes = externalRoutes
        routes.append(.builtInSpeaker)

        if !systemAudioSession.categoryOptions.contains(.defaultToSpeaker) {
            routes.append(.builtInReceiver)
        }

        let current: CallAudioRoute? = systemAudioSession.currentRoute.outputs.first.map { output -> CallAudioRoute in
            switch output.portType {
            case .builtInSpeaker:
                .builtInSpeaker
            case .builtInReceiver:
                .builtInReceiver
            default:
                .external(output)
            }
        }

        return CallAudioRouteState(
            selectedRoute: current,
            availableRoutes: routes.sorted(by: { $0.displayName > $1.displayName })
        )
    }

    func updateRouteState() {
        let newState = computeState()

        guard newState != routeStateSubject.value else {
            return
        }

        routeStateSubject.send(newState)
    }

    func applyRoute(_ route: CallAudioRoute) throws {
        audioSession.lockForConfiguration()

        defer {
            audioSession.unlockForConfiguration()
        }

        if !audioSession.isActive {
            try audioSession.setActive(true)
        }

        let availableInputs = systemAudioSession.availableInputs ?? []
        let builtInMic = availableInputs.first { $0.portType == .builtInMic }

        switch route {
        case .builtInSpeaker:
            if let builtInMic {
                try systemAudioSession.setPreferredInput(builtInMic)
            }
            try audioSession.overrideOutputAudioPort(.speaker)
        case .builtInReceiver:
            try audioSession.overrideOutputAudioPort(.none)
            if let builtInMic {
                try systemAudioSession.setPreferredInput(builtInMic)
            }
        case let .external(port):
            try audioSession.overrideOutputAudioPort(.none)

            let match = availableInputs.first {
                $0.callDeviceIdentifier == port.callDeviceIdentifier
            }
            if let match {
                try systemAudioSession.setPreferredInput(match)
            }
        }
    }
}

extension CallAudioSessionManager: CallAudioSessionManaging {
    func configureAudioSession(for callType: ChatCallType) throws {
        audioSession.lockForConfiguration()

        defer {
            audioSession.unlockForConfiguration()
        }

        try setupAudioSessionConfig(for: callType)
        try setupInitialLoudspeakerBehavior(for: callType)

        startRouteObservation()
    }

    func setEnabled(_ isEnabled: Bool, for session: AVAudioSession) {
        if isEnabled {
            // Starting WebRTC audio I/O without record permission can crash the app.
            guard recordPermissionProvider.recordPermission == .granted else {
                replaceSessionPendingPermission(with: session)
                return
            }
            audioSession.audioSessionDidActivate(session)
        } else {
            replaceSessionPendingPermission(with: nil)
            audioSession.audioSessionDidDeactivate(session)
        }
        audioSession.isAudioEnabled = isEnabled
    }

    func enableAudioIfPermitted() {
        guard
            recordPermissionProvider.recordPermission == .granted,
            let session = replaceSessionPendingPermission(with: nil)
        else {
            return
        }

        audioSession.audioSessionDidActivate(session)
        audioSession.isAudioEnabled = true
    }

    func selectRoute(_ route: CallAudioRoute) throws {
        try applyRoute(route)
    }

    func observeRouteState() -> AnyAsyncSequence<CallAudioRouteState> {
        routeStateSubject.eraseToAnyAsyncSequence()
    }
}
