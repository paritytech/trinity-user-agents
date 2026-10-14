import Foundation
import UIKit
import SubstrateSdk
import BigInt
import UniformTypeIdentifiers

extension Chat.LocalMessage.Content {
    enum FileType: UInt8 {
        case pdf
    }

    enum FileLocation: Equatable {
        case bundleName(String)
        case relativeDocumentsPath(String)
    }

    struct File: Equatable {
        let type: FileType
        let location: FileLocation
        let customName: String?
        let text: String?
        let url: URL

        init?(
            type: FileType,
            location: FileLocation,
            customName: String?,
            text: String?
        ) {
            guard let url = location.makeURL(fileType: type) else {
                return nil
            }
            self.type = type
            self.location = location
            self.customName = customName
            self.text = text
            self.url = url
        }

        var name: String {
            customName ?? url.lastPathComponent
        }
    }
}

extension Chat.LocalMessage.Content.File: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        type = try Chat.LocalMessage.Content.FileType(scaleDecoder: scaleDecoder)
        location = try Chat.LocalMessage.Content.FileLocation(scaleDecoder: scaleDecoder)
        customName = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value

        guard let url = location.makeURL(fileType: type) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.url = url
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try type.encode(scaleEncoder: scaleEncoder)
        try location.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: customName).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.FileType {
    var fileExtension: String {
        switch self {
        case .pdf: "pdf"
        }
    }

    var utType: UTType {
        switch self {
        case .pdf: .pdf
        }
    }
}

extension Chat.LocalMessage.Content.FileType: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let rawType = try UInt8(scaleDecoder: scaleDecoder)

        guard let type = Chat.LocalMessage.Content.FileType(rawValue: rawType) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = type
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.FileLocation {
    func makeURL(fileType: Chat.LocalMessage.Content.FileType) -> URL? {
        switch self {
        case let .bundleName(string):
            Bundle.main.url(
                forResource: string,
                withExtension: fileType.fileExtension
            )
        case let .relativeDocumentsPath(path):
            makeDocumentsURL(
                relativePath: path,
                fileType: fileType
            )
        }
    }

    private func makeDocumentsURL(
        relativePath: String,
        fileType: Chat.LocalMessage.Content.FileType
    ) -> URL? {
        FileManager.default.urls(
            for: .documentDirectory,
            in: .userDomainMask
        )
        .first?
        .appendingPathComponent(relativePath, conformingTo: fileType.utType)
    }
}

extension Chat.LocalMessage.Content.FileLocation: ScaleCodable {
    var scaleIndex: UInt8 {
        switch self {
        case .bundleName: 0
        case .relativeDocumentsPath: 1
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let rawType = try UInt8(scaleDecoder: scaleDecoder)

        switch rawType {
        case 0:
            self = try .bundleName(String(scaleDecoder: scaleDecoder))
        case 1:
            self = try .relativeDocumentsPath(String(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .bundleName(string):
            try string.encode(scaleEncoder: scaleEncoder)
        case let .relativeDocumentsPath(path):
            try path.encode(scaleEncoder: scaleEncoder)
        }
    }
}
