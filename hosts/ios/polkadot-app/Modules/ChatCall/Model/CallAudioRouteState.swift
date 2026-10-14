import AVFoundation

enum CallAudioRoute {
    case builtInReceiver
    case builtInSpeaker
    case external(AVAudioSessionPortDescription)
}

extension CallAudioRoute: Hashable {
    static func == (lhs: CallAudioRoute, rhs: CallAudioRoute) -> Bool {
        switch (lhs, rhs) {
        case (.builtInReceiver, .builtInReceiver),
             (.builtInSpeaker, .builtInSpeaker):
            true
        case let (.external(lhsPort), .external(rhsPort)):
            lhsPort.callDeviceIdentifier == rhsPort.callDeviceIdentifier
        default:
            false
        }
    }

    func hash(into hasher: inout Hasher) {
        switch self {
        case .builtInReceiver:
            hasher.combine(0)
        case .builtInSpeaker:
            hasher.combine(1)
        case let .external(port):
            hasher.combine(2)
            hasher.combine(port.callDeviceIdentifier)
        }
    }
}

extension AVAudioSessionPortDescription {
    /// Canonical "same physical device" key across input/output port descriptions.
    ///
    /// Bluetooth: iOS exposes one accessory under multiple profile UIDs
    /// (`<MAC>-tsco` HFP, `<MAC>-tacl` A2DP, `<MAC>-xxxxx` LE). During a
    /// `voiceChat` session the active output can come from one profile while
    /// `availableInputs` lists the other — stripping the suffix collapses them.
    ///
    /// Wired: iOS reports a wired headset as separate `headsetMic` input and
    /// `headphones` output with unrelated UIDs. Only one wired device can be
    /// connected at a time, so a shared key collapses them into one entry.
    var callDeviceIdentifier: String {
        switch portType {
        case .bluetoothA2DP,
             .bluetoothHFP,
             .bluetoothLE:
            if let dash = uid.lastIndex(of: "-") {
                return String(uid[..<dash])
            }
            return uid
        case .headphones,
             .headsetMic:
            return "wired"
        default:
            return uid
        }
    }
}

struct CallAudioRouteState: Equatable {
    let selectedRoute: CallAudioRoute?
    let availableRoutes: [CallAudioRoute]
}

extension CallAudioRouteState {
    static let unknown = CallAudioRouteState(selectedRoute: nil, availableRoutes: [])

    static let externalOutputPorts: Set<AVAudioSession.Port> = [
        .bluetoothA2DP,
        .bluetoothHFP,
        .bluetoothLE,
        .headphones,
        .headsetMic,
        .airPlay,
        .carAudio,
        .usbAudio,
        .lineOut,
        .thunderbolt,
        .HDMI,
        .displayPort
    ]

    var isUsingNonReceiver: Bool {
        switch selectedRoute {
        case .builtInReceiver,
             .none:
            false
        case .builtInSpeaker,
             .external:
            true
        }
    }
}

extension CallAudioRoute {
    var displayName: String {
        switch self {
        case .builtInReceiver:
            String(localized: .callAudioRouteBuiltInReceiver)
        case .builtInSpeaker:
            String(localized: .callAudioRouteBuiltInSpeaker)
        case let .external(port):
            port.portName
        }
    }

    var iconName: String {
        switch self {
        case .builtInReceiver:
            "iphone"
        case .builtInSpeaker:
            "speaker.wave.3.fill"
        case let .external(port):
            Self.externalIconName(portType: port.portType, name: port.portName)
        }
    }

    static func externalIconName(portType: AVAudioSession.Port, name: String) -> String {
        switch portType {
        case .bluetoothA2DP,
             .bluetoothHFP,
             .bluetoothLE:
            name.localizedCaseInsensitiveContains("airpods")
                ? "airpodspro"
                : "dot.radiowaves.left.and.right"
        case .headphones,
             .headsetMic,
             .lineOut:
            "headphones"
        case .airPlay:
            "airplayaudio"
        case .carAudio:
            "car.fill"
        case .usbAudio,
             .thunderbolt:
            "cable.connector"
        case .HDMI,
             .displayPort:
            "tv"
        default:
            "speaker.wave.2.fill"
        }
    }
}

extension CallAudioRouteState {
    var outputIconName: String {
        switch selectedRoute {
        case .builtInReceiver,
             .none:
            "speaker.wave.3.fill"
        case .builtInSpeaker,
             .external:
            selectedRoute?.iconName ?? "speaker.fill"
        }
    }
}
