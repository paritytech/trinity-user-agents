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
        let username: AnyPublisher<DotnsGatewayPallet.AccountNameRecord?, Error> = subscription(
            request: path.batchStorageRequest(mapping: nil)
        )
        let retVal: AnyPublisher<Data?, Error> = username
            .map { $0?.username }
            .eraseToAnyPublisher()

        return retVal
            .map {
                $0.flatMap { Username(rawData: $0) }
            }
            .eraseToAnyPublisher()
    }

    func username(
        for accountId: AccountId
    ) -> AnyPublisher<Username?, any Error> {
        let username: AnyPublisher<DotnsGatewayPallet.AccountNameRecord?, Error> = queryStorage(
            at: DotnsGatewayPallet.Storage.accountNames(accountId),
            params: [BytesCodable(wrappedValue: accountId)]
        )
        let retVal: AnyPublisher<Data?, Error> = username
            .tryCatch { error in
                guard case SubscriptionServiceError.noData = error else {
                    throw error
                }
                return Just<DotnsGatewayPallet.AccountNameRecord?>(nil).setFailureType(to: Error.self)
            }
            .map { $0?.username }
            .eraseToAnyPublisher()

        return retVal
            .map { $0.flatMap { Username(rawData: $0) } }
            .eraseToAnyPublisher()
    }
}
