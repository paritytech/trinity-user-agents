import Foundation
import AVFoundation
import WebRTC
import TrUAPIHost

/// Expected failures never carry platform diagnostics across UniFFI.
enum NativeMediaFailure: Error {
    case cancelled
    case denied
    case domain(NativeMediaDomainError)

    var response: NativeMediaBackendResponse {
        switch self {
        case .cancelled: .rejected(failure: .domain(error: .captureCancelled))
        case .denied: .rejected(failure: .denied)
        case let .domain(error): .rejected(failure: .domain(error: error))
        }
    }
}

final class NativeMediaVideoSource: NSObject, RTCVideoCapturerDelegate, @unchecked Sendable {
    let source: RTCVideoSource
    let track: RTCVideoTrack
    private let lock = NSLock()
    private var lastFrame: TimeInterval = 0
    private var stopped = false
    private let screen: Bool
    private var inputWidth: Int32 = 0
    private var inputHeight: Int32 = 0

    init(factory: RTCPeerConnectionFactory, screen: Bool) {
        self.screen = screen
        source = factory.videoSource(forScreenCast: screen)
        track = factory.videoTrack(with: source, trackId: UUID().uuidString)
        track.isEnabled = false
        super.init()
    }

    func capturer(_ capturer: RTCVideoCapturer, didCapture frame: RTCVideoFrame) {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return }
        if screen, frame.width > 0, frame.height > 0,
           frame.width != inputWidth || frame.height != inputHeight {
            inputWidth = frame.width; inputHeight = frame.height
            let longest = max(1280, max(frame.width, frame.height))
            source.adaptOutputFormat(toWidth: max(2, frame.width * 1280 / longest),
                height: max(2, frame.height * 1280 / longest), fps: 15)
        }
        lastFrame = ProcessInfo.processInfo.systemUptime
        source.capturer(capturer, didCapture: frame)
    }

    var state: NativeMediaTrackState {
        lock.lock(); defer { lock.unlock() }
        if stopped { return .off }
        if lastFrame == 0 { return .starting }
        return ProcessInfo.processInfo.systemUptime - lastFrame < 3 ? .live : .interrupted
    }

    func stop() {
        lock.lock(); defer { lock.unlock() }
        stopped = true; lastFrame = 0; track.isEnabled = false
    }
}

@MainActor
final class NativeMediaCamera {
    let video: NativeMediaVideoSource
    let capturer: RTCCameraVideoCapturer
    private(set) var kind: NativeMediaCameraKind = .other
    private var stopped = false

    init(factory: RTCPeerConnectionFactory) {
        video = NativeMediaVideoSource(factory: factory, screen: false)
        capturer = RTCCameraVideoCapturer(delegate: video)
    }

    func start(preference: NativeMediaCameraKind?) async throws {
        guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized else { throw NativeMediaFailure.denied }
        let devices = RTCCameraVideoCapturer.captureDevices()
        let position: AVCaptureDevice.Position = preference == .rear ? .back : .front
        guard let device = devices.first(where: { $0.position == position }) ?? devices.first else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        let preferences = VideoCapturePreferences(maxWidth: 640, maxHeight: 480, maxFps: 24)
        guard let selection = VideoCaptureStrategy(preferences: preferences).deriveParams(for: device) else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        kind = device.position == .front ? .front : (device.position == .back ? .rear : .other)
        do { try await capturer.startCapture(with: device, format: selection.format, fps: selection.fps) }
        catch { throw NativeMediaFailure.domain(.deviceUnavailable) }
        guard !stopped, !Task.isCancelled else { stop(); throw NativeMediaFailure.cancelled }
    }

    func stop() {
        stopped = true
        video.stop()
        capturer.stopCapture(completionHandler: {})
    }
}

@MainActor
final class NativeMediaScreen {
    let video: NativeMediaVideoSource
    private let capturer: RTCVideoCapturer
    private let mailbox: MediaBroadcastMailbox
    private let generation = Int64.random(in: 1...Int64.max)
    private var task: Task<Void, Never>?
    private var stopped = false
    var onStop: (() -> Void)?

    init(factory: RTCPeerConnectionFactory) throws {
        video = NativeMediaVideoSource(factory: factory, screen: true)
        capturer = RTCVideoCapturer(delegate: video)
        mailbox = try MediaBroadcastMailbox(create: true)
    }

    func start(presentation: NativeMediaPresentation) async throws {
        try mailbox.prepare(generation: generation)
        do {
            try presentation.showScreenPicker()
            let deadline = Date().addingTimeInterval(120)
            while try mailbox.status(generation: generation) != 2 {
                guard !stopped, !Task.isCancelled, Date() < deadline else { throw NativeMediaFailure.cancelled }
                try await Task.sleep(for: .milliseconds(50))
            }
            guard !stopped, !Task.isCancelled else { throw NativeMediaFailure.cancelled }
            presentation.dismissPicker()
            task = Task { [weak self] in
                var sequence: Int64 = 0
                while let self, !self.stopped, !Task.isCancelled {
                    do {
                        try self.mailbox.heartbeat(generation: self.generation)
                        if let frame = try self.mailbox.frame(after: sequence, generation: self.generation) {
                            sequence = frame.sequence
                            let angle: RTCVideoRotation
                            switch frame.rotation {
                            case 90: angle = ._90
                            case 180: angle = ._180
                            case 270: angle = ._270
                            default: angle = ._0
                            }
                            self.video.capturer(
                                self.capturer,
                                didCapture: RTCVideoFrame(
                                    buffer: RTCCVPixelBuffer(pixelBuffer: frame.buffer),
                                    rotation: angle,
                                    timeStampNs: frame.timeStampNs
                                )
                            )
                        }
                        try await Task.sleep(for: .milliseconds(40))
                    } catch {
                        if !self.stopped { self.stop(); self.onStop?() }
                        return
                    }
                }
            }
        } catch {
            stop(); presentation.dismissPicker()
            if let failure = error as? NativeMediaFailure { throw failure }
            throw NativeMediaFailure.cancelled
        }
    }

    func stop() {
        stopped = true; task?.cancel(); task = nil
        video.stop(); mailbox.stop(generation: generation)
    }
}

@MainActor
final class NativeMediaCapture {
    let intent: NativeMediaLocalTracks
    private let audio: NativeMediaAudioDevice
    var microphone: RTCAudioTrack?
    var camera: NativeMediaCamera?
    var screen: NativeMediaScreen?

    init(intent: NativeMediaLocalTracks, audio: NativeMediaAudioDevice) { self.intent = intent; self.audio = audio }

    var tracks: [RTCMediaStreamTrack?] { [microphone, camera?.video.track, screen?.video.track] }

    func setEnabled(_ enabled: Bool) { for track in tracks { track?.isEnabled = enabled } }

    func stop(except other: NativeMediaCapture? = nil) {
        if microphone !== other?.microphone { microphone?.isEnabled = false }
        if camera !== other?.camera { camera?.stop() }
        if screen !== other?.screen { screen?.stop() }
        microphone = nil; camera = nil; screen = nil
    }

    func state(interrupted: Bool) -> NativeMediaLocalState {
        let route = interrupted || audio.hasFailed ? nil : AVAudioSession.sharedInstance().currentRoute.outputs.first?.portType
        let audioRoute: NativeMediaAudioRoute?
        switch route {
        case .builtInReceiver: audioRoute = .earpiece
        case .builtInSpeaker: audioRoute = .speaker
        case .headphones, .bluetoothA2DP, .bluetoothHFP, .bluetoothLE: audioRoute = .headset
        case nil: audioRoute = nil
        default: audioRoute = .other
        }
        let microphoneState: NativeMediaTrackState
        if microphone == nil { microphoneState = .off }
        else if interrupted || audio.hasFailed || audio.isMuted || AVAudioApplication.shared.recordPermission != .granted {
            microphoneState = .interrupted
        } else if audio.microphoneIsLive { microphoneState = .live }
        else { microphoneState = audio.hasRecordedInput ? .interrupted : .starting }
        return NativeMediaLocalState(microphone: microphoneState,
            camera: camera?.video.state ?? .off, screen: screen?.video.state ?? .off,
            cameraKind: camera?.kind, audioRoute: audioRoute)
    }
}
