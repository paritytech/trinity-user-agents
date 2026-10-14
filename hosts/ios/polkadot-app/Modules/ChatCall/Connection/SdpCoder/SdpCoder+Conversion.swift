import Foundation

extension SdpCoder.MinimalCandidate.TransportType {
    init?(transportString: String) {
        switch transportString.lowercased() {
        case "tcp":
            self = .tcp
        case "udp":
            self = .udp
        default:
            return nil
        }
    }

    var stringValue: String {
        switch self {
        case .tcp:
            "TCP"
        case .udp:
            "UDP"
        }
    }
}

extension SdpCoder.MinimalCandidate.IPAddress {
    init?(addressString: String) {
        if let ipv4 = SdpCoder.IP4Address(fromString: addressString) {
            self = .ipv4(ipv4)
        } else if let ipv6 = SdpCoder.IP6Address(inputString: addressString) {
            self = .ipv6(ipv6)
        } else {
            return nil
        }
    }

    var stringValue: String {
        switch self {
        case let .ipv4(addr):
            addr.toString()
        case let .ipv6(addr):
            addr.toString()
        }
    }
}

extension SdpCoder.MinimalCandidate.CandidateType {
    init?(typeString: String) {
        switch typeString {
        case "host":
            self = .host
        case "srflx":
            self = .srflx
        case "relay":
            self = .relay
        case "prflx":
            self = .prflx
        default:
            return nil
        }
    }

    var stringValue: String {
        switch self {
        case .host:
            "host"
        case .srflx:
            "srflx"
        case .relay:
            "relay"
        case .prflx:
            "prflx"
        }
    }
}

extension SdpCoder.SdpType {
    var stringValue: String {
        switch self {
        case .offer:
            "actpass"
        case .answer:
            "active"
        }
    }
}
