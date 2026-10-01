import Foundation
import SubstrateSdk
import Individuality
import Combine

enum IdentityServiceError: Error {
    case accountNotFound
}

protocol IdentityServiceProtocol {
    /// Creates a subscription to **DotnsGateway.AccountNames** storage
    func subscribe(to accountId: AccountId) -> AnyPublisher<Username?, Error>
    /// Creates a one-time query to **DotnsGateway.AccountNames** storage
    func username(for accountId: AccountId) -> AnyPublisher<Username?, Error>
}

final class IdentityService: BaseSubscriptionService {}

extension IdentityService: IdentityServiceProtocol {
    func subscribe(
        to accountId: AccountId
    ) -> AnyPublisher<Username?, any Error> {
        let path = DotnsGatewayPallet.Storage.accountNames(accountId)
        let record: AnyPublisher<DotnsGatewayPallet.AccountNameRecord?, Error> = subscription(
            request: path.batchStorageRequest(mapping: nil)
        )

        return record
            .map { $0?.username.flatMap(Username.init(rawData:)) }
            .eraseToAnyPublisher()
    }

    func username(
        for accountId: AccountId
    ) -> AnyPublisher<Username?, any Error> {
        let record: AnyPublisher<DotnsGatewayPallet.AccountNameRecord?, Error> = queryStorage(
            at: DotnsGatewayPallet.Storage.accountNames(accountId),
            params: [BytesCodable(wrappedValue: accountId)]
        )

        return record
            .tryCatch { error in
                guard case SubscriptionServiceError.noData = error else {
                    throw error
                }
                return Just<DotnsGatewayPallet.AccountNameRecord?>(nil).setFailureType(to: Error.self)
            }
            .map { $0?.username.flatMap(Username.init(rawData:)) }
            .eraseToAnyPublisher()
    }
}
