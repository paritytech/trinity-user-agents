import Foundation
import SubstrateSdk

extension Chat.LocalMessage.Content {
    /// A body that a product posts into its own room and the host draws with native views.
    enum ProductBody: Equatable {
        case richText(ProductRichText)
        case actions(ProductActions)
        case file(ProductFile)
    }

    /// Text with media. Each media item is an `https` URL or an inline `data:` image.
    struct ProductRichText: Equatable {
        let text: String?
        let media: [String]
    }

    struct ProductActions: Equatable {
        let text: String?
        let buttons: [ProductActionButton]
        let layout: ProductActionLayout
    }

    struct ProductActionButton: Equatable {
        let actionId: String
        let title: String
    }

    enum ProductActionLayout: UInt8, Equatable {
        case column = 0
        case grid = 1
    }

    /// A file that the product serves at an `https` URL.
    struct ProductFile: Equatable {
        let url: String
        let fileName: String
        let mimeType: String
        let sizeBytes: UInt64
        let text: String?
    }
}

extension Chat.LocalMessage.Content.ProductBody {
    /// The text part of the body, or nil when the body has no text.
    var text: String? {
        switch self {
        case let .richText(richText): richText.text
        case let .actions(actions): actions.text
        case let .file(file): file.text
        }
    }
}

// MARK: - ScaleCodable

extension Chat.LocalMessage.Content.ProductBody: ScaleCodable {
    private enum Tag: UInt8 {
        case richText = 0
        case actions = 1
        case file = 2
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let rawTag = try UInt8(scaleDecoder: scaleDecoder)
        switch Tag(rawValue: rawTag) {
        case .richText:
            self = try .richText(.init(scaleDecoder: scaleDecoder))
        case .actions:
            self = try .actions(.init(scaleDecoder: scaleDecoder))
        case .file:
            self = try .file(.init(scaleDecoder: scaleDecoder))
        case .none:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .richText(content):
            try Tag.richText.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .actions(content):
            try Tag.actions.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .file(content):
            try Tag.file.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.LocalMessage.Content.ProductRichText: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        media = try [String](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try media.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductActions: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        buttons = try [Chat.LocalMessage.Content.ProductActionButton](scaleDecoder: scaleDecoder)
        let rawLayout = try UInt8(scaleDecoder: scaleDecoder)
        guard let layout = Chat.LocalMessage.Content.ProductActionLayout(rawValue: rawLayout) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.layout = layout
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try buttons.encode(scaleEncoder: scaleEncoder)
        try layout.rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductActionButton: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        actionId = try String(scaleDecoder: scaleDecoder)
        title = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try actionId.encode(scaleEncoder: scaleEncoder)
        try title.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductFile: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        url = try String(scaleDecoder: scaleDecoder)
        fileName = try String(scaleDecoder: scaleDecoder)
        mimeType = try String(scaleDecoder: scaleDecoder)
        sizeBytes = try UInt64(scaleDecoder: scaleDecoder)
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try url.encode(scaleEncoder: scaleEncoder)
        try fileName.encode(scaleEncoder: scaleEncoder)
        try mimeType.encode(scaleEncoder: scaleEncoder)
        try sizeBytes.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
    }
}
