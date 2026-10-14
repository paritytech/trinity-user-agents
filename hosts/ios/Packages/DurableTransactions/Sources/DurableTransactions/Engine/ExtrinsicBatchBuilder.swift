@preconcurrency import ExtrinsicService
import Foundation
@preconcurrency import SDKLogger
import StructuredConcurrency

/// Builds and signs every request's extrinsic up-front, so each hash is known before registration.
///
/// Requests sharing a ``DurableTxBatchKey`` are built in a single `buildExtrinsics` call so their nonces
/// are sequential — required for dependent transactions where one spends another's output. Requests
/// without a key are built independently, each with its own signer. Order is preserved so the returned
/// models align with the requests.
public struct ExtrinsicBatchBuilder {
    let operationFactory: any ExtrinsicOperationFactoryProtocol
    let logger: SDKLoggerProtocol?

    /// Public so a domain's submission policy builds its rebuilt extrinsics through the same grouping
    /// and ordering the engine uses for a first attempt, rather than reimplementing them.
    public init(operationFactory: any ExtrinsicOperationFactoryProtocol, logger: SDKLoggerProtocol?) {
        self.operationFactory = operationFactory
        self.logger = logger
    }

    public func build(_ requests: [DurableTxRequest]) async throws -> [ExtrinsicBuiltModel] {
        guard !requests.isEmpty else { return [] }

        // Batches are independent of one another, so run them concurrently rather than serialising one
        // behind another. Results are re-assembled by original request index below, so the returned order
        // is deterministic.
        let built = try await withThrowingTaskGroup(
            of: (indices: [Int], models: [ExtrinsicBuiltModel]).self
        ) { taskGroup in
            for group in Self.groupByBatchKey(requests) {
                taskGroup.addTask {
                    try await (group, buildGroup(group, of: requests))
                }
            }

            var collected: [(indices: [Int], models: [ExtrinsicBuiltModel])] = []
            for try await result in taskGroup {
                collected.append(result)
            }
            return collected
        }

        var models = [ExtrinsicBuiltModel?](repeating: nil, count: requests.count)
        for entry in built {
            for (position, requestIndex) in entry.indices.enumerated() {
                models[requestIndex] = entry.models[position]
            }
        }

        return try models.map { model in
            guard let model else { throw DurableTxError.buildIncomplete }
            return model
        }
    }
}

private extension ExtrinsicBatchBuilder {
    /// Builds one batch's extrinsics in a single indexed call, so their nonces are sequential.
    func buildGroup(_ group: [Int], of requests: [DurableTxRequest]) async throws -> [ExtrinsicBuiltModel] {
        logger?.debug("Building \(group.count) extrinsic(s)")
        let wrapper = operationFactory.buildExtrinsics(
            { builder, index in try requests[group[index]].builder(builder) },
            origin: requests[group[0]].origin,
            payingIn: nil,
            indexes: IndexSet(0 ..< group.count)
        )
        let built = try await wrapper.asyncExecute()

        guard built.count == group.count else {
            throw DurableTxError.buildIncomplete
        }
        return built
    }
}

extension ExtrinsicBatchBuilder {
    /// Groups request indices by batch key, preserving first-seen and within-group order. A request
    /// without a key is a batch of its own: the origin's identity says nothing, since an origin may be a
    /// value type boxed anew on every use.
    static func groupByBatchKey(_ requests: [DurableTxRequest]) -> [[Int]] {
        var groups: [[Int]] = []
        var indexByKey: [DurableTxBatchKey: Int] = [:]
        for (index, request) in requests.enumerated() {
            guard let key = request.batchKey else {
                groups.append([index])
                continue
            }
            if let groupIndex = indexByKey[key] {
                groups[groupIndex].append(index)
            } else {
                indexByKey[key] = groups.count
                groups.append([index])
            }
        }
        return groups
    }
}
