import Foundation
import StructuredConcurrency
import SubstrateSdk

/// Which name the runtime gives `Revive.call`'s weight limit — see ``RevivePallet/weightLimitArgument(in:)``.
public protocol ReviveCallArgumentsProviding: Sendable {
    func weightLimitArgument() async throws -> RevivePallet.WeightLimitArgument
}

public final class RuntimeReviveCallArguments: ReviveCallArgumentsProviding, @unchecked Sendable {
    private let runtimeService: any RuntimeCodingServiceProtocol

    public init(runtimeService: any RuntimeCodingServiceProtocol) {
        self.runtimeService = runtimeService
    }

    public func weightLimitArgument() async throws -> RevivePallet.WeightLimitArgument {
        let metadata = try await runtimeService.fetchCoderFactoryOperation().asyncExecute().metadata
        return RevivePallet.weightLimitArgument(in: metadata)
    }
}
