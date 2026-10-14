import AsyncExtensions
import PolkadotUI

protocol ChatExtensionWidgetProvidable {
    func widgetConfigurationStream() async throws -> AnyAsyncSequence<(any HashableContentConfiguration)?>
}

struct ChatExtensionWidgetProvider {
    let extensionId: ChatExtension.Id
    let provider: any ChatExtensionWidgetProvidable
}
