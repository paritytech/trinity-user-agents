import Operation_iOS
import Products

extension Product: Operation_iOS.Identifiable {
    public var identifier: String { id }
}

extension Product {
    var extensionId: ChatExtension.Id {
        identifier
    }
}
