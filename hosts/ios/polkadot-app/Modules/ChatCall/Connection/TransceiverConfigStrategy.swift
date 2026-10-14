import Foundation
import WebRTC

protocol TransceiverConfigStrategyProtocol {
    func configureVideoTransceiver(
        for videoTrack: RTCVideoTrack?,
        on connection: RTCPeerConnection
    )
}

final class TransceiverConfigStrategy {
    let peerConnectionFactory: RTCPeerConnectionFactory
    let videoProfile: RTCVideoProfile
    let logger: LoggerProtocol

    init(
        peerConnectionFactory: RTCPeerConnectionFactory,
        videoProfile: RTCVideoProfile,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.peerConnectionFactory = peerConnectionFactory
        self.videoProfile = videoProfile
        self.logger = logger
    }
}

extension TransceiverConfigStrategy: TransceiverConfigStrategyProtocol {
    func configureVideoTransceiver(
        for videoTrack: RTCVideoTrack?,
        on connection: RTCPeerConnection
    ) {
        if let videoTrack {
            guard let transceiver = makeSendReceiveVideoTransceiver(for: videoTrack, on: connection) else {
                return
            }

            configureVideoCodecPreferences(transceiver)
            configureVideoSender(transceiver.sender)
        } else {
            // video is negotiated even without a local track for better UX when a user enables it
            guard let transceiver = makeReceiveOnlyVideoTransceiver(on: connection) else {
                return
            }

            configureVideoCodecPreferences(transceiver)
        }
    }
}

private extension TransceiverConfigStrategy {
    func makeSendReceiveVideoTransceiver(
        for videoTrack: RTCVideoTrack,
        on connection: RTCPeerConnection
    ) -> RTCRtpTransceiver? {
        if let transceiver = reusableRemoteOfferVideoTransceiver(on: connection) {
            transceiver.sender.track = videoTrack
            setVideoTransceiver(transceiver, direction: .sendRecv)
            return transceiver
        }

        let initConfig = RTCRtpTransceiverInit()
        initConfig.direction = .sendRecv
        initConfig.streamIds = ["stream0"]

        guard let transceiver = connection.addTransceiver(with: videoTrack, init: initConfig) else {
            logger.error("Failed to add video transceiver")
            return nil
        }

        return transceiver
    }

    func makeReceiveOnlyVideoTransceiver(on connection: RTCPeerConnection) -> RTCRtpTransceiver? {
        let transceiver: RTCRtpTransceiver?
        if let reusableTransceiver = reusableRemoteOfferVideoTransceiver(on: connection) {
            transceiver = reusableTransceiver
            setVideoTransceiver(reusableTransceiver, direction: .recvOnly)
        } else {
            let initConfig = RTCRtpTransceiverInit()
            initConfig.direction = .recvOnly
            transceiver = connection.addTransceiver(
                of: .video,
                init: initConfig
            )
        }

        if transceiver == nil {
            logger.error("Failed to add video transceiver")
        }

        return transceiver
    }

    func setVideoTransceiver(_ transceiver: RTCRtpTransceiver, direction: RTCRtpTransceiverDirection) {
        var error: NSError?
        transceiver.setDirection(direction, error: &error)

        if let error {
            logger.error("Failed to set video transceiver direction to \(direction): \(error)")
        }
    }

    func reusableRemoteOfferVideoTransceiver(on connection: RTCPeerConnection) -> RTCRtpTransceiver? {
        guard connection.remoteDescription != nil else {
            return nil
        }

        return connection.transceivers.first { transceiver in
            transceiver.mediaType == .video && transceiver.sender.track == nil
        }
    }

    func configureVideoCodecPreferences(_ transceiver: RTCRtpTransceiver) {
        let codecs = peerConnectionFactory
            .rtpSenderCapabilities(forKind: kRTCMediaStreamTrackKindVideo)
            .codecs

        let preferredCodecs = makeH264FirstVideoCodecs(from: codecs)

        guard !preferredCodecs.isEmpty else {
            logger.warning("No video codec preferences available")
            return
        }

        do {
            try transceiver.setCodecPreferences(preferredCodecs, error: ())
        } catch {
            logger.error("Failed to set video codec preferences: \(error)")
        }
    }

    func makeH264FirstVideoCodecs(from codecs: [RTCRtpCodecCapability]) -> [RTCRtpCodecCapability] {
        var h264Codecs: [RTCRtpCodecCapability] = []
        var vp8Codecs: [RTCRtpCodecCapability] = []
        var otherCodecs: [RTCRtpCodecCapability] = []

        for codec in codecs {
            if codec.name.isSameCodec(as: kRTCH264CodecName as String) {
                h264Codecs.append(codec)
            } else if codec.name.isSameCodec(as: kRTCVp8CodecName as String) {
                vp8Codecs.append(codec)
            } else {
                otherCodecs.append(codec)
            }
        }

        return h264Codecs + vp8Codecs + otherCodecs
    }

    func configureVideoSender(_ sender: RTCRtpSender) {
        let parameters = sender.parameters
        parameters.degradationPreference = NSNumber(value: videoProfile.degradationPreference.rawValue)

        var encodings = parameters.encodings
        let encoding = encodings.first ?? RTCRtpEncodingParameters()
        encoding.maxBitrateBps = NSNumber(value: videoProfile.maxBitrateBps)
        encoding.maxFramerate = NSNumber(value: videoProfile.captureFps)

        if encodings.isEmpty {
            encodings = [encoding]
        } else {
            encodings[0] = encoding
        }

        parameters.encodings = encodings
        sender.parameters = parameters
    }
}

private extension String {
    func isSameCodec(as other: String) -> Bool {
        caseInsensitiveCompare(other) == .orderedSame
    }
}
