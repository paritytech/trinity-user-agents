import Foundation
import SubstrateSdk

// MARK: - Host Handshake Proposal (decoded from QR)

enum HandshakeProposal: ScaleDecodable {
    // swiftlint:disable:next identifier_name
    case v1(DataV1)
    // swiftlint:disable:next identifier_name
    case v2(DataV2)

    private enum Constants {
        static var scheme: String { AppConfig.DeepLink.scheme }
        static let host = "pair"
        static let queryItem = "handshake"
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .v1(DataV1(scaleDecoder: scaleDecoder))
        case 1:
            self = try .v2(DataV2(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    init?(url: URL, logger: LoggerProtocol = Logger.shared) {
        guard url.scheme == Constants.scheme else {
            logger.debug("Unsupported url scheme: \(url.scheme ?? "nil")")
            return nil
        }

        guard url.host() == Constants.host else {
            logger.debug("Unsupported url host: \(url.host() ?? "nil")")
            return nil
        }

        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            logger.debug("Unsupported url components for url: \(url)")
            return nil
        }

        guard let queryItems = components.queryItems, !queryItems.isEmpty else {
            logger.debug("Invalid query items count: \(components.queryItems?.count ?? 0)")
            return nil
        }

        guard queryItems[0].name == Constants.queryItem else {
            logger.debug("Invalid query item name: \(queryItems[0].name)")
            return nil
        }

        guard let hexValue = queryItems[0].value else {
            logger.debug("Missing query item value")
            return nil
        }

        do {
            let scaleData = try Data(hexString: hexValue)
            let decoder = try ScaleDecoder(data: scaleData)
            try self.init(scaleDecoder: decoder)
        } catch {
            logger.error("Failed to decode host data: \(error)")
            return nil
        }
    }

    var statementAccountId: Data {
        switch self {
        case let .v1(dataV1):
            dataV1.statementStorePublicKey
        case let .v2(dataV2):
            dataV2.statementAccountId
        }
    }
}

// MARK: - V1 Proposal

extension HandshakeProposal {
    struct DataV1: ScaleDecodable {
        let statementStorePublicKey: Data
        let encryptionPublicKey: Data
        let metadata: String
        let hostVersion: String?
        let osType: String?
        let osVersion: String?

        init(scaleDecoder: any ScaleDecoding) throws {
            statementStorePublicKey = try scaleDecoder.readAndConfirm(count: 32)
            encryptionPublicKey = try scaleDecoder.readAndConfirm(count: 32)
            metadata = try String(scaleDecoder: scaleDecoder)
            hostVersion = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
            osType = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
            osVersion = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        }
    }
}

// MARK: - V2 Proposal

extension HandshakeProposal {
    struct DataV2: ScaleDecodable {
        let statementAccountId: Data
        let encryptionPublicKey: Data
        let metadata: [HandshakeMetadataKey: String]

        init(scaleDecoder: any ScaleDecoding) throws {
            statementAccountId = try scaleDecoder.readAndConfirm(count: 32)
            encryptionPublicKey = try scaleDecoder.readAndConfirm(count: 32)

            let entries = try [HandshakeMetadataEntry](scaleDecoder: scaleDecoder)
            metadata = Dictionary(entries.map { ($0.key, $0.value) }) { _, last in last }
        }
    }
}

// MARK: - V2 Metadata

enum HandshakeMetadataKey: Hashable, ScaleDecodable {
    case custom(String)

    /// Host user-readable name. e.g. "Polkadot Desktop"
    case hostName

    /// Host version, possibly in semver format
    case hostVersion

    /// CID from bulletin chain
    case hostIcon

    /// OS name or Browser name, depends on host type. e.g. "MacOS" or "Firefox"
    case platformType

    /// OS or Browser version. e.g. "26.1" for MacOS or "192.32" for chromium based browsers
    case platformVersion

    /// Location in latitude;longitude format. e.g. "20.379610618468575;15.1903828953423643"
    case location

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .custom(String(scaleDecoder: scaleDecoder))
        case 1:
            self = .hostName
        case 2:
            self = .hostVersion
        case 3:
            self = .hostIcon
        case 4:
            self = .platformType
        case 5:
            self = .platformVersion
        case 6:
            self = .location
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }
}

struct HandshakeMetadataEntry: ScaleDecodable {
    let key: HandshakeMetadataKey
    let value: String

    init(scaleDecoder: any ScaleDecoding) throws {
        key = try HandshakeMetadataKey(scaleDecoder: scaleDecoder)
        value = try String(scaleDecoder: scaleDecoder)
    }
}

extension HandshakeProposal.DataV2 {
    var hostName: String? { metadata[.hostName] }
    var hostVersion: String? { metadata[.hostVersion] }
    var hostIconCID: String? { metadata[.hostIcon] }
    var platformType: String? { metadata[.platformType] }
    var platformVersion: String? { metadata[.platformVersion] }
}

// MARK: - Unified Device Data

struct HandshakeDeviceData {
    let statementAccountId: Data
    let encryptionPublicKey: Data
    let hostVersion: String?
    let platformType: String?
    let platformVersion: String?
}

extension HandshakeDeviceData {
    init(from data: HandshakeProposal.DataV1) {
        statementAccountId = data.statementStorePublicKey
        encryptionPublicKey = data.encryptionPublicKey
        hostVersion = data.hostVersion
        platformType = data.osType
        platformVersion = data.osVersion
    }

    init(from data: HandshakeProposal.DataV2) {
        statementAccountId = data.statementAccountId
        encryptionPublicKey = data.encryptionPublicKey
        hostVersion = data.hostVersion
        platformType = data.platformType
        platformVersion = data.platformVersion
    }
}

extension HandshakeProposal {
    var deviceData: HandshakeDeviceData {
        switch self {
        case let .v1(data):
            HandshakeDeviceData(from: data)
        case let .v2(data):
            HandshakeDeviceData(from: data)
        }
    }
}
