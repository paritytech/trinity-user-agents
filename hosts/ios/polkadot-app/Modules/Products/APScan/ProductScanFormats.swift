import AVFoundation
import TrUAPIHost

/// Translates between the formats a product names and the ones AVFoundation reads.
enum ProductScanFormats {
    /// UPC-A has no type of its own: AVFoundation reads it as EAN-13 with a
    /// leading zero, which is how products expect it.
    private static let metadataTypes: [CodeFormat: [AVMetadataObject.ObjectType]] = [
        .qr: [.qr],
        .aztec: [.aztec],
        .dataMatrix: [.dataMatrix],
        .pdf417: [.pdf417],
        .ean13: [.ean13],
        .ean8: [.ean8],
        .upcE: [.upce],
        .code128: [.code128],
        .code39: [.code39, .code39Mod43],
        .code93: [.code93],
        .itf: [.itf14, .interleaved2of5],
        .codabar: [.codabar]
    ]

    static func metadataTypes(for formats: [CodeFormat]) -> [AVMetadataObject.ObjectType] {
        formats.flatMap { metadataTypes[$0] ?? [] }
    }

    /// Nil for a type no product can ask for.
    static func format(of type: AVMetadataObject.ObjectType) -> CodeFormat? {
        metadataTypes.first { $0.value.contains(type) }?.key
    }
}
