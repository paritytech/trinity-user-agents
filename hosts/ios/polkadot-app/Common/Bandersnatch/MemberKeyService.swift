import Foundation
import BandersnatchApi
import SubstrateSdk

enum MemberKeyServiceError: Error {
    case entropyFailed(error: EntropyError)
    case memberKeyFailed(error: Error)
}

struct MemberKeyData: Codable {
    let entropy: Data
    let memberKey: Data
}

protocol MemberKeyServicing: AnyObject {
    func deriveNewMemberKey() -> Result<MemberKeyData, MemberKeyServiceError>
    func deriveNewMemberKey(from entropy: Data) -> Result<MemberKeyData, MemberKeyServiceError>
}

final class MemberKeyService: MemberKeyServicing {
    private let entropyGenerator: EntropyGenerating
    private let bandersnatchApi: BandersnatchApi.Type
    private let logger: LoggerProtocol

    init(
        entropyGenerator: EntropyGenerating = EntropyGenerator(),
        bandersnatchApi: BandersnatchApi.Type = BandersnatchApi.self,
        logger: Logger = .shared
    ) {
        self.entropyGenerator = entropyGenerator
        self.bandersnatchApi = bandersnatchApi
        self.logger = logger
    }

    func deriveNewMemberKey() -> Result<MemberKeyData, MemberKeyServiceError> {
        let entropyResult = entropyGenerator.generateEntropy(of: BandersnatchApi.entropySize)
        switch entropyResult {
        case let .success(entropy):
            do {
                let memberKey = try BandersnatchApi.deriveMemberKey(from: entropy)
                logger.debug("Generated memberKey as hex: \(memberKey.toHex())")
                return .success(.init(entropy: entropy, memberKey: memberKey))
            } catch {
                return .failure(.memberKeyFailed(error: error))
            }
        case let .failure(error):
            return .failure(.entropyFailed(error: error))
        }
    }

    func deriveNewMemberKey(from entropy: Data) -> Result<MemberKeyData, MemberKeyServiceError> {
        do {
            let memberKey = try BandersnatchApi.deriveMemberKey(from: entropy)
            logger.debug("Generated memberKey as hex: \(memberKey.toHex())")
            return .success(.init(entropy: entropy, memberKey: memberKey))
        } catch {
            return .failure(.memberKeyFailed(error: error))
        }
    }
}
