import Foundation
import PolkadotUI
import AsyncExtensions

protocol ChatExtensionActionProvidable {
    func contentConfiguration() async throws -> AnyAsyncSequence<(any HashableContentConfiguration)?>
}
