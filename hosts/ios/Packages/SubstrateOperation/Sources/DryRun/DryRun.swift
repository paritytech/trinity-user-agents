import Foundation
import SubstrateSdk
import XcmDefinition

public enum DryRun {
    static let apiName = "DryRunApi"

    public struct ForwardedXcm: Decodable {
        public let location: XcmUni.VersionedLocation
        public let messages: [XcmUni.VersionedMessage]

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            location = try container.decode(XcmUni.VersionedLocation.self)
            messages = try container.decode([XcmUni.VersionedMessage].self)
        }
    }

    public typealias CallExecutionResult = Substrate.Result<JSON, JSON>

    public struct CallDryRunEffects: Decodable {
        public let executionResult: CallExecutionResult
        public let emittedEvents: [Event]
        public let localXcm: XcmUni.VersionedMessage?
        public let forwardedXcms: [ForwardedXcm]
    }

    public typealias CallResult = Substrate.Result<CallDryRunEffects, JSON>

    public enum DryRunError<R>: Error {
        case failure(JSON)
        case execution(JSON)
    }

    public typealias CallDryRunError = DryRunError<JSON>

    public typealias XcmExecutionResult = Xcm.Outcome<Substrate.WeightV2, JSON>

    public struct XcmDryRunEffects: Decodable {
        public let executionResult: XcmExecutionResult
        public let emittedEvents: [Event]
        public let forwardedXcms: [ForwardedXcm]
    }

    public typealias XcmResult = Substrate.Result<XcmDryRunEffects, JSON>

    public typealias XcmDryRunError = DryRunError<JSON>
}

public extension DryRun.CallResult {
    func ensureSuccessExecution() throws -> DryRun.CallDryRunEffects {
        let effects = try ensureOkOrError { DryRun.CallDryRunError.failure($0) }

        try effects.executionResult.ensureOkOrError { DryRun.CallDryRunError.execution($0) }

        return effects
    }
}

public extension DryRun.CallDryRunEffects {
    func xcmVersion() -> Xcm.Version? {
        forwardedXcms.first?.location.version
    }
}

public extension DryRun.XcmResult {
    func ensureSuccessExecution() throws -> DryRun.XcmDryRunEffects {
        let effects = try ensureOkOrError { DryRun.XcmDryRunError.failure($0) }

        try effects.executionResult.ensureCompleteOrError { DryRun.XcmDryRunError.execution($0) }

        return effects
    }
}
