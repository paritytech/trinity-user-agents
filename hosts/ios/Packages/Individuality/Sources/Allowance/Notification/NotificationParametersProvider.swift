import ChainStore
import Foundation
import SDKLogger
import SubstrateSdk

public protocol NotificationParametersProviding: Sendable {
    func currentPeriod() async throws -> UInt32
    func maxStatementSize() async throws -> Int
}

public enum NotificationParametersError: Error {
    case zeroPeriodDuration
}

public final class NotificationParametersProvider: @unchecked Sendable {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let chainTimeProvider: ChainTimeProviding
    private let statementStorageParameters: StatementStorageParametersProviding
    private let logger: SDKLoggerProtocol

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        chainTimeProvider: ChainTimeProviding,
        statementStorageParameters: StatementStorageParametersProviding,
        logger: SDKLoggerProtocol
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.chainTimeProvider = chainTimeProvider
        self.statementStorageParameters = statementStorageParameters
        self.logger = logger
    }
}

extension NotificationParametersProvider: NotificationParametersProviding {
    public func currentPeriod() async throws -> UInt32 {
        let duration: StringCodable<UInt32> = try await constant(.notificationPeriodDuration)

        return try await Self.period(atSeconds: chainTimeProvider.nowSeconds(), duration: duration.wrappedValue)
    }

    public func maxStatementSize() async throws -> Int {
        guard let allowance = try await statementStorageParameters.notificationAllowance() else {
            logger.info("NotificationAllowance is not set on \(chainId); using the runtime default")

            return Int(ResourcesPallet.StatementAllowance.notificationDefault.maxSize)
        }

        return Int(allowance.maxSize)
    }
}

extension NotificationParametersProvider {
    static func period(atSeconds nowSeconds: UInt64, duration: UInt32) throws -> UInt32 {
        guard duration > 0 else {
            throw NotificationParametersError.zeroPeriodDuration
        }

        return UInt32(clamping: nowSeconds / UInt64(duration))
    }
}

private extension NotificationParametersProvider {
    func constant<T: Decodable>(_ path: ResourcesPallet.Constants) async throws -> T {
        try await chainRegistry.getRuntimeCodingServiceOrError(for: chainId).fetchConstant(path: path())
    }
}
