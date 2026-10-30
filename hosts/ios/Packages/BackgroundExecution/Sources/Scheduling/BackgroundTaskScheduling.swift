import Foundation

public struct BackgroundTaskConstraints: Sendable, Equatable {
    public let requiresNetworkConnectivity: Bool
    public let requiresExternalPower: Bool

    public init(requiresNetworkConnectivity: Bool, requiresExternalPower: Bool) {
        self.requiresNetworkConnectivity = requiresNetworkConnectivity
        self.requiresExternalPower = requiresExternalPower
    }
}

public struct BackgroundTaskRequest: Sendable, Equatable {
    public let identifier: String
    public let earliestBeginDate: Date?
    public let constraints: BackgroundTaskConstraints

    public init(identifier: String, earliestBeginDate: Date?, constraints: BackgroundTaskConstraints) {
        self.identifier = identifier
        self.earliestBeginDate = earliestBeginDate
        self.constraints = constraints
    }
}

public protocol BackgroundTaskScheduling: Sendable {
    /// - Returns: `false` when the identifier is missing from `BGTaskSchedulerPermittedIdentifiers`.
    func register(
        identifier: String,
        launchHandler: @escaping @Sendable (any BackgroundTaskHandle) -> Void
    ) -> Bool

    func submit(_ request: BackgroundTaskRequest) throws

    func pendingRequestIdentifiers() async -> [String]
}

public protocol BackgroundTaskHandle: AnyObject, Sendable {
    var expirationHandler: (@Sendable () -> Void)? { get set }

    func setTaskCompleted(success: Bool)
}
