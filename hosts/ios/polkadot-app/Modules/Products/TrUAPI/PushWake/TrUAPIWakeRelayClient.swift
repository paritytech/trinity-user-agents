import Foundation
import TrUAPIHost

/// What identifies this device to the push relay.
struct WakeRelayDevice: Codable, Equatable, Sendable {
    /// `apns`, `apns-sandbox` or, on a simulator, `simctl`.
    let platform: String
    /// The APNs token as hex, or the simulator udid for `simctl`.
    let token: String
    /// The bundle id of the app.
    let app: String
}

/// The push relay client of the app (VERBOSITY-WASM-PRODUCT A1).
///
/// The core gives it the wake topics of every product and the key of the device. The client adds the device and
/// sends the registration to the relay endpoint. No product reaches it. The relay sends a wake push for each new
/// statement on a registered topic.
///
/// No relay with APNs credentials exists yet (06-OPEN-ITEMS O27), so the app installs this client only where an
/// endpoint is configured: on a simulator under the E2E harness, with the reference relay `truapi-host wake-relay`.
final class TrUAPIWakeRelayClient: WakeRelayHostBridge, @unchecked Sendable {
    private struct Registration: Encodable {
        let device: WakeRelayDevice
        let key: String
        let topics: [String]
    }

    private let endpoint: URL
    private let device: WakeRelayDevice
    private let session: URLSession
    private let logger: LoggerProtocol

    init(
        endpoint: URL,
        device: WakeRelayDevice,
        session: URLSession = .shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.endpoint = endpoint
        self.device = device
        self.session = session
        self.logger = logger
    }

    /// The client for the configured relay, or nil when this build has no relay.
    static func configured(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundleId: String? = Bundle.main.bundleIdentifier
    ) -> TrUAPIWakeRelayClient? {
        #if targetEnvironment(simulator)
            guard
                let url = environment["TRUAPI_IOS_E2E_WAKE_RELAY_URL"].flatMap(URL.init(string:)),
                let udid = environment["SIMULATOR_UDID"],
                let bundleId
            else {
                return nil
            }
            return TrUAPIWakeRelayClient(
                endpoint: url,
                device: WakeRelayDevice(platform: "simctl", token: udid, app: bundleId)
            )
        #else
            return nil
        #endif
    }

    /// The JSON body of a registration.
    func body(for registration: WakeRelayRegistration) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(
            Registration(
                device: device,
                key: Self.hex(registration.key),
                topics: registration.topics.map(Self.hex)
            )
        )
    }

    func registerWakeTopics(registration: WakeRelayRegistration) async throws {
        var request = URLRequest(url: endpoint.appendingPathComponent("v1/registrations"))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try body(for: registration)
        request.timeoutInterval = 10

        let (_, response) = try await session.data(for: request)
        guard let status = (response as? HTTPURLResponse)?.statusCode, (200 ..< 300).contains(status) else {
            throw TrUAPIWakeRelayError.refused
        }
        logger.info("[truapi] wake relay has \(registration.topics.count) topics of this device")
    }

    private static func hex(_ data: Data) -> String {
        "0x" + data.map { String(format: "%02x", $0) }.joined()
    }
}

enum TrUAPIWakeRelayError: Error, LocalizedError {
    case refused

    var errorDescription: String? {
        "the wake relay refused the registration"
    }
}
