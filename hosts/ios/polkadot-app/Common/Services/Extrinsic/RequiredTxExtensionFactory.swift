import Foundation
import SubstrateSdk

///     Transaction extensions that must always be applied to outgoing extrinsics regardless of
///     per-transaction overrides. They are applied last so they take precedence over both default
///     and transaction-supplied extensions sharing the same identifier.
final class RequiredTxExtensionFactory {}

extension RequiredTxExtensionFactory: ExtrinsicTransactionExtensionMaking {
    func createExtensions() -> [TransactionExtending] {
        []
    }

    func createCoders(for _: RuntimeMetadataProtocol) -> [TransactionExtensionCoding] {
        []
    }
}
