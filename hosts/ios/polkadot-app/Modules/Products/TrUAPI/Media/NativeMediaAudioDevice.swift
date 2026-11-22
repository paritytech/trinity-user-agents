import Foundation
import AVFoundation
import AudioToolbox
import WebRTC

/// The default WebRTC iOS ADM opens full-duplex VoiceProcessingIO for playout.
/// Media instead owns an output-only RemoteIO unit until committed microphone
/// intent allows input. PCM stays entirely inside native WebRTC callback blocks.
final class NativeMediaAudioDevice: NSObject, RTCAudioDevice, @unchecked Sendable {
    private let lock = NSLock()
    private var delegate: RTCAudioDeviceDelegate?
    private var desiredMicrophone = false
    private var desiredActive = false
    private var desiredLease = false
    private var desiredMuted = false
    private var lastInput: UInt64 = 0
    private var receivedInput = false
    private var failed = false

    // Everything below is confined to the ADM's dispatch context. Audio render
    // closures capture immutable blocks and buffers configured before start.
    private var unit: AUAudioUnit?
    private var routeObserver: NSObjectProtocol?
    private var microphone = false
    private var active = false
    private var leased = false
    private(set) var isPlaying = false
    private(set) var isRecording = false
    private(set) var deviceInputSampleRate: Double = 48_000
    private(set) var deviceOutputSampleRate: Double = 48_000
    private(set) var inputIOBufferDuration: TimeInterval = 0.01
    private(set) var outputIOBufferDuration: TimeInterval = 0.01
    private(set) var inputLatency: TimeInterval = 0
    private(set) var outputLatency: TimeInterval = 0
    var inputNumberOfChannels: Int { 1 }
    var outputNumberOfChannels: Int { 1 }
    var isInitialized: Bool { currentDelegate != nil }
    var isPlayoutInitialized: Bool { isInitialized }
    var isRecordingInitialized: Bool { isInitialized }

    var microphoneIsLive: Bool {
        lock.lock(); defer { lock.unlock() }
        return desiredLease && desiredMicrophone && !desiredMuted && desiredActive && !failed && lastInput != 0
            && DispatchTime.now().uptimeNanoseconds - lastInput < 3_000_000_000
    }

    var hasFailed: Bool { lock.lock(); defer { lock.unlock() }; return failed }
    var isMuted: Bool { lock.lock(); defer { lock.unlock() }; return desiredMuted }
    var hasRecordedInput: Bool { lock.lock(); defer { lock.unlock() }; return receivedInput }

    private var currentDelegate: RTCAudioDeviceDelegate? {
        lock.lock(); defer { lock.unlock() }; return delegate
    }

    func configure(microphone: Bool, active: Bool) throws {
        lock.lock()
        if !microphone || !desiredMicrophone { receivedInput = false }
        desiredMicrophone = microphone; desiredActive = active; desiredLease = true
        let delegate = delegate
        lock.unlock()
        guard let delegate else { throw NativeMediaFailure.domain(.deviceUnavailable) }
        delegate.dispatchSync { _ = self.applyDesired() }
        guard !hasFailed else { throw NativeMediaFailure.domain(.deviceUnavailable) }
    }

    func setActive(_ active: Bool) {
        lock.lock(); desiredActive = active; let delegate = delegate; lock.unlock()
        delegate?.dispatchSync { _ = self.applyDesired() }
    }

    func setMuted(_ muted: Bool) {
        lock.lock(); desiredMuted = muted; let delegate = delegate; lock.unlock()
        delegate?.dispatchSync { _ = self.applyDesired() }
    }

    func stop() {
        lock.lock()
        desiredLease = false; desiredActive = false; desiredMicrophone = false; lastInput = 0
        receivedInput = false
        let delegate = delegate
        lock.unlock()
        delegate?.dispatchSync { _ = self.applyDesired() }
    }

    func initialize(with delegate: RTCAudioDeviceDelegate) -> Bool {
        lock.lock()
        guard self.delegate == nil else { lock.unlock(); return false }
        self.delegate = delegate
        lock.unlock()
        routeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification, object: nil, queue: nil
        ) { [weak self] _ in
            guard let self, let delegate = self.currentDelegate else { return }
            delegate.dispatchAsync { [weak self] in
                guard let self, self.currentDelegate === delegate else { return }
                _ = self.rebuild()
            }
        }
        return applyDesired()
    }

    func terminateDevice() -> Bool {
        stopUnit()
        if let routeObserver { NotificationCenter.default.removeObserver(routeObserver) }
        routeObserver = nil
        lock.lock(); delegate = nil; lastInput = 0; lock.unlock()
        isPlaying = false; isRecording = false
        return true
    }

    func initializePlayout() -> Bool { isInitialized }
    func initializeRecording() -> Bool { isInitialized }
    // Stream demand gates WebRTC's PCM blocks; the committed call independently
    // owns hardware, so adding/removing a peer must not restart the audio unit.
    func startPlayout() -> Bool { isPlaying = true; return !hasFailed }
    func stopPlayout() -> Bool { isPlaying = false; return true }
    func startRecording() -> Bool { isRecording = true; return !hasFailed }
    func stopRecording() -> Bool { isRecording = false; return true }

    private func applyDesired() -> Bool {
        lock.lock()
        let microphone = desiredMicrophone && !desiredMuted, active = desiredActive, leased = desiredLease
        lock.unlock()
        guard microphone != self.microphone || active != self.active || leased != self.leased else { return !hasFailed }
        self.microphone = microphone; self.active = active; self.leased = leased
        return rebuild()
    }

    private func stopUnit() {
        unit?.stopHardware()
        unit?.deallocateRenderResources()
        unit?.inputHandler = nil; unit?.outputProvider = nil
        unit = nil
        lock.lock(); lastInput = 0; lock.unlock()
        currentDelegate?.notifyAudioInputInterrupted()
        currentDelegate?.notifyAudioOutputInterrupted()
    }

    private func rebuild() -> Bool {
        stopUnit()
        guard leased, active, let delegate = currentDelegate else {
            lock.lock(); failed = false; lock.unlock()
            return true
        }
        let session = AVAudioSession.sharedInstance()
        let records = microphone
        do {
            if microphone, AVAudioApplication.shared.recordPermission != .granted {
                throw NativeMediaFailure.denied
            }
            guard session.sampleRate > 0 else { throw NativeMediaFailure.domain(.deviceUnavailable) }
            let audioUnit = try AUAudioUnit(componentDescription: AudioComponentDescription(
                componentType: kAudioUnitType_Output,
                componentSubType: microphone ? kAudioUnitSubType_VoiceProcessingIO : kAudioUnitSubType_RemoteIO,
                componentManufacturer: kAudioUnitManufacturer_Apple,
                componentFlags: 0, componentFlagsMask: 0
            ))
            unit = audioUnit
            audioUnit.isInputEnabled = records
            audioUnit.isOutputEnabled = true
            audioUnit.maximumFramesToRender = 4096
            guard let format = AVAudioFormat(commonFormat: .pcmFormatInt16,
                sampleRate: session.sampleRate, channels: 1, interleaved: true) else {
                throw NativeMediaFailure.domain(.deviceUnavailable)
            }
            deviceInputSampleRate = format.sampleRate; deviceOutputSampleRate = format.sampleRate
            inputIOBufferDuration = session.ioBufferDuration; outputIOBufferDuration = session.ioBufferDuration
            inputLatency = microphone ? session.inputLatency : 0; outputLatency = session.outputLatency
            delegate.notifyAudioInputParametersChange()
            delegate.notifyAudioOutputParametersChange()
            try audioUnit.inputBusses[0].setFormat(format)
            let playout = delegate.getPlayoutData
            audioUnit.outputProvider = { [weak self] flags, timestamp, frames, bus, data in
                let status = playout(flags, timestamp, bus, frames, data)
                if status != noErr, let self {
                    self.lock.lock(); self.failed = true; self.lock.unlock()
                }
                return status
            }
            if records {
                try audioUnit.outputBusses[1].setFormat(format)
                let render = audioUnit.renderBlock
                let deliver = delegate.deliverRecordedData
                let record: RTCAudioDeviceRenderRecordedDataBlock = { flags, timestamp, bus, frames, data, _ in
                    render(flags, timestamp, frames, bus, data, nil)
                }
                audioUnit.inputHandler = { [weak self] flags, timestamp, frames, bus in
                    let status = deliver(flags, timestamp, bus, frames, nil, nil, record)
                    guard let self else { return }
                    self.lock.lock()
                    if status == noErr {
                        self.lastInput = DispatchTime.now().uptimeNanoseconds
                        self.receivedInput = true
                    }
                    else { self.failed = true }
                    self.lock.unlock()
                }
            }
            // The committed call owns its output path through receive-only and
            // peer gaps, keeping CallKit background audio alive without opening
            // input. WebRTC supplies decoded output (silence before playout).
            // Committed microphone intent owns input even before a peer joins;
            // WebRTC discards input until a sending stream is started.
            lock.lock(); failed = false; lock.unlock()
            try audioUnit.allocateRenderResources()
            try audioUnit.startHardware()
            return true
        } catch {
            stopUnit()
            lock.lock(); failed = true; lock.unlock()
            return false
        }
    }
}
