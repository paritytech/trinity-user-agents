import Foundation
import TrUAPIHost

/// The Trinity stacks the app bundles, and which one each build runs.
///
/// Each stack ships as two resources: `<resource>.link`, the stack's pointer,
/// and `<resource>.json`, its signed descriptor. The app target's "Bundle
/// Trinity stacks" build phase copies both from the repository's `stacks/`,
/// which `scripts/update-stacks.sh` refreshes. The core checks the descriptor
/// against the pointer's key when the runtime starts.
enum TrUAPIBundledStack {
    enum Error: Swift.Error, Equatable {
        case missingResource(name: String)
    }

    /// One stack the app bundles, and whether it may use Polkadot's genesis
    /// hashes and the `dot` TLD.
    struct Definition {
        let resource: String
        let allowsReservedValues: Bool

        /// The stack read from `bundle`, as the core takes it.
        func load(from bundle: Bundle = .main) throws -> BundledStack {
            let pointer = try String(contentsOf: url(of: "link", in: bundle), encoding: .utf8)
            return try BundledStack(
                pointer: pointer.trimmingCharacters(in: .whitespacesAndNewlines),
                descriptor: Data(contentsOf: url(of: "json", in: bundle)),
                reservedValues: ReservedValues(allowed: allowsReservedValues)
            )
        }
    }

    static let previewnet = Definition(resource: "trinity-stack-previewnet", allowsReservedValues: false)

    static let paseoNextV2 = Definition(resource: "trinity-stack-paseo-next-v2", allowsReservedValues: false)

    /// The only stack allowed Polkadot's reserved values.
    static let polkadot = Definition(resource: "trinity-stack-polkadot", allowsReservedValues: true)

    /// The stack this build runs.
    static var current: Definition {
        #if UNSTABLE
            previewnet
        #elseif NIGHTLY || SAFETYNET
            paseoNextV2
        #else
            polkadot
        #endif
    }
}

private extension TrUAPIBundledStack.Definition {
    func url(of fileExtension: String, in bundle: Bundle) throws -> URL {
        guard let url = bundle.url(forResource: resource, withExtension: fileExtension) else {
            throw TrUAPIBundledStack.Error.missingResource(name: "\(resource).\(fileExtension)")
        }
        return url
    }
}
