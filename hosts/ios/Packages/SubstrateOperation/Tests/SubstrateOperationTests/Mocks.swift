import Foundation
import FoundationExt
import os
import SubstrateSdk

@testable import SubstrateOperation

struct RawArg: ScaleEncodable {
    let byte: UInt8

    init(_ byte: UInt8) { self.byte = byte }

    func encode(scaleEncoder: ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: Data([byte]))
    }
}

final class MutableClock: DateProviding, Sendable {
    private let state: OSAllocatedUnfairLock<Date>

    init(now: Date) {
        state = OSAllocatedUnfairLock(initialState: now)
    }

    func set(_ date: Date) {
        state.withLock { $0 = date }
    }

    func read() async -> Date {
        state.withLock { $0 }
    }
}

/// Records how often the executor is hit and returns a distinct value per call by default, so a
/// cache hit (no new call) is observable as an unchanged value and call count.
final class SpyViewFunctionExecutor: ViewFunctionExecuting, Sendable {
    private let count = OSAllocatedUnfairLock(initialState: 0)

    /// The value returned for the Nth call (1-based). Distinct per call by default. The caller owns
    /// the concrete type here — it must match the `T` the fetch is decoded into.
    private let valueForCall: @Sendable (Int) -> Any

    init(valueForCall: @escaping @Sendable (Int) -> Any = { UInt32($0) }) {
        self.valueForCall = valueForCall
    }

    var callCount: Int {
        count.withLock { $0 }
    }

    func call<T: Decodable>(
        viewFunction _: ViewFunctionCodingPath,
        chainId _: ChainId,
        args _: [ScaleEncodable]
    ) async throws -> T {
        let index = count.withLock {
            $0 += 1
            return $0
        }

        guard let typed = valueForCall(index) as? T else {
            throw SpyError.typeMismatch
        }
        return typed
    }
}

enum SpyError: Error {
    case typeMismatch
}
