import Foundation
import SubstrateSdk
import StructuredConcurrency

public extension NestedCallMapperProtocol {
    func hasMachingCall(
        in rawCallData: Data,
        runtimeCodingService: RuntimeCodingServiceProtocol,
        matchingClosure: (AnyRuntimeCall, RuntimeJsonContext?) -> Bool
    ) async throws -> Bool {
        let codingFactory = try await runtimeCodingService.fetchCoderFactoryOperation().asyncExecute()
        let decoder = try codingFactory.createDecoder(from: rawCallData)
        let call: JSON = try decoder.read(type: KnownType.call.name)
        let context = codingFactory.createRuntimeJsonContext()

        let nodeResult: NestedCallNode<AnyRuntimeCall> = try mapRuntimeCall(call: call, context: context)

        return nodeResult.calls.contains { call in
            matchingClosure(call, context)
        }
    }
}
