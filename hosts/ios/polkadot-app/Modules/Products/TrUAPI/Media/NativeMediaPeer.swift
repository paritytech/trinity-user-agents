import Foundation
import WebRTC
import TrUAPIHost
import StructuredConcurrency

/// Retry only peer construction: failures after a peer exists must still close that peer normally.
func makeNativeMediaPeerConnection(
    configuration: RTCConfiguration,
    create: (RTCConfiguration) throws -> RTCPeerConnection?
) throws -> RTCPeerConnection? {
    try Task.checkCancellation()
    do {
        if let connection = try create(configuration) { return connection }
    } catch {
        try Task.checkCancellation()
        guard !configuration.iceServers.isEmpty else { throw error }
    }
    try Task.checkCancellation()
    guard !configuration.iceServers.isEmpty else { return nil }
    configuration.iceServers = []
    return try create(configuration)
}

let nativeMediaIceGatheringDeadline: Duration = .seconds(1)
let nativeMediaLocalCandidateLimit = 256

/// Actor-confined. A description carries the gathered candidates; only candidates
/// absent from that snapshot may subsequently become individual V1 ICE messages.
/// Continual gathering has a per-description budget; optional excess is dropped.
struct NativeMediaLocalIce {
    private struct Key: Hashable {
        let mline: Int32
        let fields: [String]
    }
    private var active = true
    private var buffering = true
    private var pending: [RTCIceCandidate] = []
    private var advertised: Set<Key> = []
    private var ufrags: [Int32: String] = [:]

    mutating func begin() {
        active = true
        buffering = true
        pending.removeAll()
    }

    mutating func candidate(_ candidate: RTCIceCandidate) -> Bool {
        guard active else { return false }
        if buffering {
            if pending.count < nativeMediaLocalCandidateLimit { pending.append(candidate) }
            return false
        }
        guard advertised.count < nativeMediaLocalCandidateLimit else { return false }
        let fields = candidate.sdp.split(whereSeparator: \.isWhitespace)
        if let index = fields.firstIndex(of: "ufrag"), index + 1 < fields.count,
           let expected = ufrags[candidate.sdpMLineIndex], fields[index + 1] != expected {
            return false // Delayed callback from the generation replaced by a restart.
        }
        return advertised.insert(Self.key(candidate.sdpMLineIndex, fields)).inserted
    }

    mutating func publish(_ sdp: String) -> [RTCIceCandidate] {
        advertised.removeAll(keepingCapacity: true)
        ufrags.removeAll(keepingCapacity: true)
        var mline: Int32 = -1
        var sessionUfrag: String?
        for line in sdp.split(whereSeparator: \.isNewline) {
            if line.hasPrefix("m=") {
                mline += 1
                ufrags[mline] = sessionUfrag
            } else if line.hasPrefix("a=ice-ufrag:") {
                let ufrag = String(line.dropFirst("a=ice-ufrag:".count))
                if mline < 0 { sessionUfrag = ufrag } else { ufrags[mline] = ufrag }
            } else if mline >= 0, line.hasPrefix("a=candidate:"),
                      advertised.count < nativeMediaLocalCandidateLimit {
                advertised.insert(Self.key(mline, line.dropFirst(2).split(whereSeparator: \.isWhitespace)))
            }
        }
        buffering = false
        let candidates = pending
        pending = []
        return candidates.filter { candidate($0) }
    }

    mutating func cancel() {
        active = false
        pending.removeAll()
        advertised.removeAll()
        ufrags.removeAll()
    }

    private static func key(_ mline: Int32, _ fields: [Substring]) -> Key {
        // SDP and callback serializers differ in ufrag/generation/network
        // metadata. Credentials are checked separately; retain ICE identity.
        var identity = fields.prefix(8).map(String.init)
        var index = 8
        while index + 1 < fields.count {
            let name = fields[index]
            if name == "raddr" || name == "rport" || name == "tcptype" {
                identity.append(String(name))
                identity.append(String(fields[index + 1]))
            }
            index += 2
        }
        return Key(mline: mline, fields: identity)
    }
}

/// Only this trusted adapter sees descriptions/candidates. The existing low-level
/// wrapper supplies serialized peer operations; no Chat signaling is involved.
actor NativeMediaPeer {
    let sessionId: Data
    let participantId: Data
    private let wrapper: AsyncPeerConnectionWrapper
    private let polite: Bool
    private let negotiationQueue = SerialOperationQueue()
    private let emit: @Sendable (MediaBackendEvent) -> Void
    private let pictures: @Sendable ([RTCVideoTrack?]) -> Void
    private var observers: [Task<Void, Never>] = []
    private var closed = false
    private var makingOffer = false
    private var ignoreOffer = false
    private var needsNegotiation = false
    private var applyingDescription = false
    private var connected = false
    private var connectedOnce = false
    private var observationRevision: UInt64 = 0
    private var pendingCandidates: [RTCIceCandidate] = []
    private var slots: [RTCRtpTransceiver] = []
    private var localTracks: [RTCMediaStreamTrack?] = [nil, nil, nil]
    private var negotiatedMids: [String] = []
    private var packetObservations: [String: (UInt64, TimeInterval)] = [:]
    private var lastRemoteState: MediaRemoteState?
    private var localIce = NativeMediaLocalIce()
    private var gatheringWait: Task<Void, Error>?

    init(connection: RTCPeerConnection, sessionId: Data, participantId: Data, offerer: Bool,
         emit: @escaping @Sendable (MediaBackendEvent) -> Void,
         pictures: @escaping @Sendable ([RTCVideoTrack?]) -> Void) {
        self.sessionId = sessionId; self.participantId = participantId
        self.emit = emit; self.pictures = pictures; polite = !offerer
        wrapper = AsyncPeerConnectionWrapper(connection: connection, logger: Logger.shared)
    }

    func start(tracks: [RTCMediaStreamTrack?], offerer: Bool) async throws {
        try await negotiationQueue.run { [self] in try await startLocked(tracks: tracks, offerer: offerer) }
    }

    private func startLocked(tracks: [RTCMediaStreamTrack?], offerer: Bool) async throws {
        guard !closed, tracks.count == 3 else { throw NativeMediaFailure.cancelled }
        // addTransceiver slots are not reused for an initial incoming offer.
        // Only the offerer creates them; the answerer adopts the offered slots.
        if offerer {
            let configuration = RTCRtpTransceiverInit()
            configuration.direction = .recvOnly
            let kinds: [RTCRtpMediaType] = [.audio, .video, .video]
            var created: [RTCRtpTransceiver] = []
            try await wrapper.modify { connection in
                for kind in kinds {
                    guard let slot = connection.addTransceiver(of: kind, init: configuration) else {
                        throw NativeMediaFailure.domain(.deviceUnavailable)
                    }
                    created.append(slot)
                }
            }
            guard !closed else { throw NativeMediaFailure.cancelled }
            slots = created
        }
        try await replaceLocked(tracks: tracks)
        observe()
        if offerer { try await makeOffer() }
    }

    func replace(tracks: [RTCMediaStreamTrack?]) async throws {
        try await negotiationQueue.run { [self] in try await replaceLocked(tracks: tracks) }
    }

    private func replaceLocked(tracks: [RTCMediaStreamTrack?]) async throws {
        guard !closed, tracks.count == 3 else { throw NativeMediaFailure.cancelled }
        localTracks = tracks
        // A commit before the first offer records intent without allocating
        // duplicate answerer slots or generating an empty offer.
        if !slots.isEmpty { try await bindLocalTracks() }
    }

    private func bindLocalTracks() async throws {
        guard !closed, slots.count == 3 else { throw NativeMediaFailure.cancelled }
        let slots = slots, tracks = localTracks
        try await wrapper.modify { _ in
            for index in 0..<3 {
                slots[index].sender.track = tracks[index]
                var error: NSError?
                slots[index].setDirection(tracks[index] == nil ? .recvOnly : .sendRecv, error: &error)
                if error != nil { throw NativeMediaFailure.domain(.deviceUnavailable) }
            }
        }
    }

    private func adoptSlots() async throws {
        var adopted: [RTCRtpTransceiver] = []
        var mids: [String] = []
        try await wrapper.modify { connection in
            let transceivers = connection.transceivers
            guard transceivers.count == 3, transceivers[0].mediaType == .audio,
                  transceivers[1].mediaType == .video, transceivers[2].mediaType == .video else {
                throw NativeMediaFailure.domain(.invalidState)
            }
            for slot in transceivers {
                let candidate: String? = slot.mid
                guard let mid = candidate, !mid.isEmpty, !slot.isStopped, !mids.contains(mid) else {
                    throw NativeMediaFailure.domain(.invalidState)
                }
                mids.append(mid)
            }
            adopted = transceivers
        }
        guard !closed else { throw NativeMediaFailure.cancelled }
        guard negotiatedMids.isEmpty || negotiatedMids == mids else {
            throw NativeMediaFailure.domain(.invalidState)
        }
        // Fixed authenticated m-line order, never stream/track labels.
        slots = adopted; negotiatedMids = mids
        try await bindLocalTracks()
    }

    func renegotiate() async throws {
        try await negotiationQueue.run { [self] in try await makeOffer() }
    }

    private func makeOffer() async throws {
        guard !closed else { throw NativeMediaFailure.cancelled }
        guard slots.count == 3 else { needsNegotiation = true; return }
        guard await wrapper.currentSignalingState() == .stable else { needsNegotiation = true; return }
        needsNegotiation = false; makingOffer = true
        defer { makingOffer = false }
        let description = try await wrapper.offer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil))
        guard !closed else { throw NativeMediaFailure.cancelled }
        try await publishLocalDescription(description, kind: .offer)
    }

    func apply(_ description: MediaDescription) async throws {
        try await negotiationQueue.run { [self] in try await applyLocked(description) }
    }

    private func applyLocked(_ description: MediaDescription) async throws {
        guard !closed else { throw NativeMediaFailure.cancelled }
        let offer = description.kind == .offer
        let signalingState = await wrapper.currentSignalingState()
        let collision = offer && (makingOffer || signalingState != .stable)
        ignoreOffer = !polite && collision
        if ignoreOffer { return }
        applyingDescription = true
        observationRevision += 1
        defer { applyingDescription = false }
        if collision {
            localIce.cancel()
            try await wrapper.setLocalDescription(RTCSessionDescription(type: .rollback, sdp: ""))
        }
        guard !closed else { throw NativeMediaFailure.cancelled }
        try await wrapper.setRemoteDescription(RTCSessionDescription(type: offer ? .offer : .answer, sdp: description.sdp))
        guard !closed else { throw NativeMediaFailure.cancelled }
        // SRD may fire receiver events before it returns; they are deferred
        // until all three offered slots and committed local tracks are bound.
        try await adoptSlots()
        let candidates = pendingCandidates; pendingCandidates.removeAll()
        for candidate in candidates { try await wrapper.addRemoteCandidate(candidate) }
        if offer {
            let answer = try await wrapper.answer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil))
            guard !closed else { throw NativeMediaFailure.cancelled }
            try await publishLocalDescription(answer, kind: .answer)
        }
        applyingDescription = false
        await publishRemote()
        if needsNegotiation { try await makeOffer() }
    }

    func add(_ candidate: MediaIceCandidate) async throws {
        try await negotiationQueue.run { [self] in try await addLocked(candidate) }
    }

    private func addLocked(_ candidate: MediaIceCandidate) async throws {
        guard !closed else { throw NativeMediaFailure.cancelled }
        guard !ignoreOffer else { return }
        let value = RTCIceCandidate(sdp: candidate.candidate, sdpMLineIndex: Int32(candidate.mlineIndex ?? 0), sdpMid: candidate.mid)
        if await wrapper.hasRemoteDescription() {
            try await wrapper.addRemoteCandidate(value)
        } else {
            guard pendingCandidates.count < 128 else { throw NativeMediaError.EventOverflow }
            pendingCandidates.append(value)
        }
    }

    private func publishLocalDescription(_ description: RTCSessionDescription, kind: MediaDescriptionKind) async throws {
        localIce.begin()
        var published = false
        defer { if !published { localIce.cancel() } }
        try await wrapper.setLocalDescription(description)
        guard !closed else { throw NativeMediaFailure.cancelled }
        try Task.checkCancellation()
        try await waitForGathering()
        var current: RTCSessionDescription?
        try await wrapper.modify { connection in current = connection.localDescription }
        try Task.checkCancellation()
        guard !closed else { throw NativeMediaFailure.cancelled }
        guard let current else { throw NativeMediaFailure.domain(.invalidState) }
        let late = localIce.publish(current.sdp)
        emit(.description(sessionId: sessionId, participantId: participantId,
            description: MediaDescription(kind: kind, sdp: current.sdp)))
        published = true
        for candidate in late { emitCandidate(candidate) }
    }

    private func waitForGathering() async throws {
        let wait = Task { [wrapper] in
            try await withThrowingTaskGroup(of: Void.self) { group in
                defer { group.cancelAll() }
                group.addTask {
                    try await Task.sleep(for: nativeMediaIceGatheringDeadline)
                }
                group.addTask {
                    var complete = false
                    try await wrapper.modify { connection in complete = connection.iceGatheringState == .complete }
                    if complete { return }
                    for try await state in wrapper.iceGatheringState.eraseToAnyAsyncSequence() {
                        try Task.checkCancellation()
                        guard state == .complete else { continue }
                        // The subject can still contain Complete from before a
                        // restart. Only the live native state finishes this wait.
                        try await wrapper.modify { connection in complete = connection.iceGatheringState == .complete }
                        if complete { return }
                    }
                }
                try await group.next()
            }
        }
        gatheringWait = wait
        defer { gatheringWait = nil; wait.cancel() }
        try await withTaskCancellationHandler {
            try await wait.value
        } onCancel: {
            wait.cancel()
        }
        try Task.checkCancellation()
    }

    private func observe() {
        observers.append(Task { [weak self, wrapper] in
            do {
                for try await event in wrapper.candidates.eraseToAnyAsyncSequence() {
                    guard !Task.isCancelled else { return }
                    if case let .add(candidate) = event { await self?.candidate(candidate) }
                }
            } catch { await self?.failed() }
        })
        observers.append(Task { [weak self, wrapper] in
            do {
                for try await state in wrapper.iceConnectionState.eraseToAnyAsyncSequence() {
                    guard !Task.isCancelled else { return }
                    if let state { await self?.connectivity(state) }
                }
            } catch { await self?.failed() }
        })
        observers.append(Task { [weak self, wrapper] in
            do {
                for try await _ in wrapper.rtpReceivers.eraseToAnyAsyncSequence() {
                    guard !Task.isCancelled else { return }
                    await self?.publishRemote()
                }
            } catch { await self?.failed() }
        })
        observers.append(Task { [weak self] in
            while !Task.isCancelled {
                await self?.publishRemote()
                do { try await Task.sleep(for: .seconds(1)) } catch { return }
            }
        })
    }

    private func candidate(_ candidate: RTCIceCandidate) {
        guard !closed, candidate.sdpMLineIndex >= 0, candidate.sdpMLineIndex < 3,
              localIce.candidate(candidate) else { return }
        emitCandidate(candidate)
    }

    private func emitCandidate(_ candidate: RTCIceCandidate) {
        emit(.iceCandidate(sessionId: sessionId, participantId: participantId,
            candidate: MediaIceCandidate(candidate: candidate.sdp, mid: candidate.sdpMid,
                mlineIndex: UInt16(candidate.sdpMLineIndex))))
    }

    private func connectivity(_ state: RTCIceConnectionState) async {
        guard !closed else { return }
        let value: MediaBackendPeerState
        switch state {
        case .connected, .completed: value = .connected; connected = true; connectedOnce = true
        case .disconnected: value = .reconnecting; connected = false
        case .failed: value = .failed; connected = false
        case .closed: value = .closed; connected = false
        default: value = .connecting; connected = false
        }
        emit(.peerStateChanged(sessionId: sessionId, participantId: participantId, state: value))
        await publishRemote()
    }

    private func publishRemote() async {
        guard !closed, !applyingDescription, slots.count == 3 else { return }
        observationRevision += 1
        let revision = observationRevision
        let slots = slots, connected = connected, connectedOnce = connectedOnce
        var observations = packetObservations
        var states: [MediaTrackState] = [.off, .off, .off]
        var videos: [RTCVideoTrack?] = [nil, nil]
        do {
            try await wrapper.modify { connection in
                let report: RTCStatisticsReport = await withCheckedContinuation { continuation in
                    connection.statistics { continuation.resume(returning: $0) }
                }
                let now = ProcessInfo.processInfo.systemUptime
                for index in 0..<3 {
                    var direction = RTCRtpTransceiverDirection.inactive
                    guard slots[index].currentDirection(&direction) else { continue }
                    let candidate: String? = slots[index].mid
                    guard let mid = candidate, !mid.isEmpty else { continue }
                    var count: UInt64 = 0
                    for statistic in report.statistics.values where statistic.type == "inbound-rtp"
                            && statistic.values["mid"] as? String == mid {
                        if let packets = statistic.values["packetsReceived"] as? NSNumber {
                            count &+= packets.uint64Value
                        }
                    }
                    if count > 0, count != observations[mid]?.0 { observations[mid] = (count, now) }
                    let receiving = direction == .sendRecv || direction == .recvOnly
                    if receiving {
                        let track = slots[index].receiver.track
                        if !connected { states[index] = connectedOnce ? .interrupted : .starting }
                        else if track?.readyState != .live { states[index] = .interrupted }
                        else if let observation = observations[mid], observation.1 != 0 {
                            states[index] = now - observation.1 < 5 ? .live : .interrupted
                        } else { states[index] = .starting }
                        if index > 0, states[index] == .live { videos[index - 1] = track as? RTCVideoTrack }
                    } else if let previous = observations[mid] {
                        // Preserve the counter across Off so a re-enabled slot
                        // cannot mistake old packets for newly resumed capture.
                        observations[mid] = (previous.0, 0)
                    }
                }
            }
        } catch { return }
        guard !closed, observationRevision == revision else { return }
        packetObservations = observations
        let state = MediaRemoteState(microphone: states[0], camera: states[1], screen: states[2])
        pictures(videos)
        if lastRemoteState != state {
            lastRemoteState = state
            emit(.remoteStateChanged(sessionId: sessionId, participantId: participantId, state: state))
        }
    }

    private func failed() {
        guard !closed else { return }
        emit(.peerStateChanged(sessionId: sessionId, participantId: participantId, state: .failed))
    }

    func close() async {
        guard !closed else { return }
        closed = true
        gatheringWait?.cancel()
        gatheringWait = nil
        localIce.cancel()
        for observer in observers { observer.cancel() }
        observers.removeAll(); pendingCandidates.removeAll()
        localTracks = [nil, nil, nil]; slots.removeAll(); negotiatedMids.removeAll()
        pictures([nil, nil])
        await wrapper.close()
    }
}
