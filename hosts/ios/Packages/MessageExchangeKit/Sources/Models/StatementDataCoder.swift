import Foundation
import SubstrateSdk
import CryptoKit
import SDKLogger

public enum StatementDataDecodingError: Error {
    case decodingFailed
    case decryptionFailed
}

public enum StatementDataEncodingError: Error {
    case encodingFailed
    case encryptionFailed
}

public enum StatementDataDecodingResult<M: MessageExchange.CodableMessage> {
    case statementData(StatementData<M>)
    case requestId(String, Error)
}

public protocol StatementDataCoding {
    func decodeFromScaleEncodedPayload<M: MessageExchange.CodableMessage>(
        _ payload: Data,
        senderAccountId: Data?,
        route: PeerSessionRoute
    ) throws -> StatementDataDecodingResult<M>

    func encodeToScaleEncodedPayload(
        _ statementData: StatementData<some MessageExchange.CodableMessage>,
        route: PeerSessionRoute
    ) throws -> Data
}

public final class StatementDataCoder: StatementDataCoding {
    private let encryptor: MessageExchangeEncrypting
    private let logger: SDKLoggerProtocol?

    public init(
        encryptor: MessageExchangeEncrypting,
        logger: SDKLoggerProtocol?
    ) {
        self.encryptor = encryptor
        self.logger = logger
    }

    public func decodeFromScaleEncodedPayload<M: MessageExchange.CodableMessage>(
        _ payload: Data,
        senderAccountId _: Data?,
        route _: PeerSessionRoute
    ) throws -> StatementDataDecodingResult<M> {
        let encryptedData: Data
        let decryptedData: Data

        do {
            let decoder = try ScaleDecoder(data: payload)
            encryptedData = try Data(scaleDecoder: decoder)
        } catch {
            throw StatementDataDecodingError.decodingFailed
        }

        do {
            decryptedData = try encryptor.decrypt(encryptedData)
        } catch {
            throw StatementDataDecodingError.decryptionFailed
        }

        do {
            let decoder = try ScaleDecoder(data: decryptedData)
            let statementData = try StatementData<M>(scaleDecoder: decoder)

            return .statementData(statementData)
        } catch {
            do {
                // try to decode just requestId
                let decoder = try ScaleDecoder(data: decryptedData)
                let fallbackStatement = try StatementFallbackRequestData(scaleDecoder: decoder)
                return .requestId(fallbackStatement.requestId, error)
            } catch {
                throw StatementDataDecodingError.decodingFailed
            }
        }
    }

    public func encodeToScaleEncodedPayload(
        _ statementData: StatementData<some MessageExchange.CodableMessage>,
        route _: PeerSessionRoute
    ) throws -> Data {
        let statementScaleData: Data
        let encryptedData: Data
        let result: Data

        do {
            statementScaleData = try statementData.scaleEncoded()
        } catch {
            throw StatementDataEncodingError.encodingFailed
        }

        do {
            encryptedData = try encryptor.encrypt(statementScaleData)
        } catch {
            throw StatementDataEncodingError.encryptionFailed
        }

        do {
            result = try encryptedData.scaleEncoded()
        } catch {
            throw StatementDataEncodingError.encodingFailed
        }

        return result
    }
}

struct StatementFallbackRequestData: ScaleDecodable {
    let requestId: String

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        guard index == 0 else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        requestId = try String(scaleDecoder: scaleDecoder)
    }
}
