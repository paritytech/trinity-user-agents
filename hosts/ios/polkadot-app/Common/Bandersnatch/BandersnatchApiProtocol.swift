import BandersnatchApi
import Foundation

// TODO: Maybe change BandersnatchApi to class so we can use protocol conformance and mock it in the app for unit tests

protocol BandersnatchApiProtocol {
    static var memberKeySize: Int { get }
    static var entropySize: Int { get }
    static func deriveMemberKey(from entropy: Data) throws -> Data
    static func createProof(
        from entropy: Data,
        members: [Data],
        message: Data,
        context: Data,
        domainSize: BandersnatchApi.RingDomainSize
    ) throws -> Data

    static func sign(entropy: Data, message: Data) throws -> Data
}

extension BandersnatchApi: BandersnatchApiProtocol {}
