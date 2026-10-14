import Foundation
import SubstrateSdk

public extension RuntimeCoderFactoryProtocol {
    /// Decode SCALE-encoded call bytes into a runtime call, or `nil` if the
    /// bytes do not decode as a known call. This is a probe — the caller
    /// decides the fallback (e.g. show the raw bytes), so it never throws.
    func decodeRuntimeCall(from data: Data) -> RuntimeCall<JSON>? {
        guard let decoder = try? createDecoder(from: data) else {
            return nil
        }

        return try? decoder.read(
            of: KnownType.call.name,
            with: createRuntimeJsonContext().toRawContext()
        )
    }
}
