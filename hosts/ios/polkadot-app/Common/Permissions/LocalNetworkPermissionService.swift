import Darwin
import Foundation
import Keystore_iOS

enum LocalNetworkPermissionRequestResult {
    case requested
    case alreadyRequested
}

protocol LocalNetworkPermissionServicing: Sendable {
    @discardableResult
    func requestPermissionIfNeeded() async -> LocalNetworkPermissionRequestResult
}

actor LocalNetworkPermissionService: LocalNetworkPermissionServicing {
    // Share one actor so different flows cannot start concurrent probes
    static let shared = LocalNetworkPermissionService()

    private enum Constants {
        static let discardServicePort: UInt16 = 9
    }

    private let settingsManager: SettingsManagerProtocol
    private let logger: LoggerProtocol

    init(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.settingsManager = settingsManager
        self.logger = logger
    }

    @discardableResult
    func requestPermissionIfNeeded() -> LocalNetworkPermissionRequestResult {
        let wasRequested = settingsManager.value(for: .localNetworkPermissionRequested)
        logger.debug("Local network permission persisted requested state: \(wasRequested)")

        guard !wasRequested else {
            logger.debug("Skipping local network permission probe: already requested")
            return .alreadyRequested
        }

        logger.debug("Starting local network permission probe")

        let attemptedConnections = triggerLocalNetworkPrivacyAlert()

        guard attemptedConnections > 0 else {
            logger.warning("Local network permission probe skipped: no connections attempted")
            return .requested
        }

        settingsManager.set(value: true, for: .localNetworkPermissionRequested)
        logger.debug("Local network permission probe finished: \(attemptedConnections) connections attempted")

        return .requested
    }
}

private extension LocalNetworkPermissionService {
    func triggerLocalNetworkPrivacyAlert() -> Int {
        let addresses = selectedLinkLocalIPv6Addresses()
        var attemptedConnections = 0

        guard !addresses.isEmpty else {
            logger.warning("Local network permission probe found no link-local IPv6 addresses")
            return attemptedConnections
        }

        for address in addresses {
            let socketDescriptor = socket(AF_INET6, SOCK_DGRAM, 0)

            guard socketDescriptor >= 0 else {
                logger.warning("Local network permission probe failed to create socket: errno \(errno)")
                continue
            }

            defer { close(socketDescriptor) }
            attemptedConnections += 1

            withUnsafePointer(to: address) { addressPointer in
                addressPointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
                    _ = connect(
                        socketDescriptor,
                        socketAddress,
                        socklen_t(socketAddress.pointee.sa_len)
                    )
                }
            }
        }

        return attemptedConnections
    }

    func selectedLinkLocalIPv6Addresses() -> [sockaddr_in6] {
        // Use two randomized destinations per interface to improve the best-effort probe
        // without targeting the device itself.
        let firstRandomHost = (0 ..< 8).map { _ in UInt8.random(in: 0 ... 255) }
        let secondRandomHost = (0 ..< 8).map { _ in UInt8.random(in: 0 ... 255) }

        return Array(
            ipv6AddressesOfBroadcastCapableInterfaces()
                .filter(isIPv6AddressLinkLocal)
                .map {
                    var address = $0
                    address.sin6_port = Constants.discardServicePort.bigEndian
                    return address
                }
                .map {
                    [
                        setIPv6LinkLocalAddressHostPart(of: $0, to: firstRandomHost),
                        setIPv6LinkLocalAddressHostPart(of: $0, to: secondRandomHost)
                    ]
                }
                .joined()
        )
    }

    func setIPv6LinkLocalAddressHostPart(
        of address: sockaddr_in6,
        to hostPart: [UInt8]
    ) -> sockaddr_in6 {
        precondition(hostPart.count == 8)

        var result = address
        withUnsafeMutableBytes(of: &result.sin6_addr) { buffer in
            buffer[8...].copyBytes(from: hostPart)
        }

        return result
    }

    func isIPv6AddressLinkLocal(_ address: sockaddr_in6) -> Bool {
        address.sin6_addr.__u6_addr.__u6_addr8.0 == 0xFE &&
            (address.sin6_addr.__u6_addr.__u6_addr8.1 & 0xC0) == 0x80
    }

    func ipv6AddressesOfBroadcastCapableInterfaces() -> [sockaddr_in6] {
        var addressList: UnsafeMutablePointer<ifaddrs>?

        guard getifaddrs(&addressList) == 0, let firstAddress = addressList else {
            return []
        }

        defer { freeifaddrs(firstAddress) }

        return sequence(first: firstAddress, next: { $0.pointee.ifa_next })
            .compactMap { interfaceAddress in
                guard
                    (interfaceAddress.pointee.ifa_flags & UInt32(bitPattern: IFF_BROADCAST)) != 0,
                    let socketAddress = interfaceAddress.pointee.ifa_addr,
                    socketAddress.pointee.sa_family == AF_INET6,
                    socketAddress.pointee.sa_len >= MemoryLayout<sockaddr_in6>.size
                else {
                    return nil
                }

                return UnsafeRawPointer(socketAddress).loadUnaligned(as: sockaddr_in6.self)
            }
    }
}
