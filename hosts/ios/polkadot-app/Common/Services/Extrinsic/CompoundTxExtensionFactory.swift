import Foundation
import SubstrateSdk
import ChainRegistry

///     Aggregates extensions and coders from a default factory and a required factory.
///     Required extensions are concatenated after default ones so that, when the substrate-sdk
///     builder dedupes by extension identifier, the required version always wins.
final class CompoundTxExtensionFactory {
    private let defaultFactory: ExtrinsicTransactionExtensionMaking
    private let requiredFactory: ExtrinsicTransactionExtensionMaking

    init(
        defaultFactory: ExtrinsicTransactionExtensionMaking = DefaultTxExtensionFactory(),
        requiredFactory: ExtrinsicTransactionExtensionMaking = RequiredTxExtensionFactory()
    ) {
        self.defaultFactory = defaultFactory
        self.requiredFactory = requiredFactory
    }
}

extension CompoundTxExtensionFactory: ExtrinsicTransactionExtensionMaking {
    func createExtensions() -> [TransactionExtending] {
        defaultFactory.createExtensions() + requiredFactory.createExtensions()
    }

    func createCoders(for metadata: RuntimeMetadataProtocol) -> [TransactionExtensionCoding] {
        defaultFactory.createCoders(for: metadata) + requiredFactory.createCoders(for: metadata)
    }
}

extension CompoundTxExtensionFactory: SignedExtensionCodersFactoryProtocol {}
