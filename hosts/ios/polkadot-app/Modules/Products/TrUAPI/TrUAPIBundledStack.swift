import Foundation
import TrUAPIHost

/// The Trinity stacks the app bundles, and which one each build runs. The core
/// takes every chain it uses, the network it reports and the TLD the wallet's
/// reserved identities derive under from the running stack's descriptor.
///
/// Each descriptor is a signed copy under `Stacks/`, kept byte for byte as
/// published. The core verifies it against its link's key when the runtime
/// starts, so a copy refreshed without its link stops the runtime.
enum TrUAPIBundledStack {
    enum Error: Swift.Error, Equatable {
        case missingDescriptor(resource: String)
    }

    /// One stack the app bundles: its descriptor resource, the link it was
    /// published under, and whether it may use Polkadot's genesis hashes and
    /// the `dot` TLD, which only the mainnet build allows.
    struct Definition {
        let descriptorResource: String
        let pointer: String
        let allowsReservedValues: Bool

        /// The stack with its descriptor read from `bundle`, as the core takes it.
        func load(from bundle: Bundle = .main) throws -> BundledStack {
            guard let url = bundle.url(forResource: descriptorResource, withExtension: "json") else {
                throw Error.missingDescriptor(resource: descriptorResource)
            }
            return try BundledStack(
                pointer: pointer,
                descriptor: Data(contentsOf: url),
                allowsReservedValues: allowsReservedValues
            )
        }
    }

    static let previewnet = Definition(
        descriptorResource: "trinity-stack-previewnet",
        pointer: "polkadotapp://stack?key=0x4d43362f03d5511632a210632d1fb7760b5a663580f12ffca34619f79357e239"
            + "&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json",
        allowsReservedValues: false
    )

    static let paseoNextV2 = Definition(
        descriptorResource: "trinity-stack-paseo-next-v2",
        pointer: "polkadotapp://stack?key=0x2b89d8ca74aa2e11ab55f5af1168cd6929b88d1998aecd979b42cfc1078d5377"
            + "&url=https%3A%2F%2Fstack-paseo-next.parity-testnet.parity.io%2F.well-known%2Ftrinity-stack.json",
        allowsReservedValues: false
    )

    static let polkadot = Definition(
        descriptorResource: "trinity-stack-polkadot",
        pointer: "polkadotapp://stack?key=0xdcddb708759dade2e3850368ce0add2072f424ba53f1a2128f3339d646cffe28"
            + "&url=https%3A%2F%2Fstack.api.pcf.network%2F.well-known%2Ftrinity-stack.json",
        allowsReservedValues: true
    )

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
