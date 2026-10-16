import AVFoundation
import TrUAPIHost

/// Translates between the formats a product names and the ones AVFoundation reads.
enum ProductScanFormats {
    static func metadataTypes(for formats: [CodeFormat]) -> [AVMetadataObject.ObjectType] {
        formats.flatMap(metadataTypes(for:))
    }

    /// Nil for a type no product can ask for.
    static func format(of type: AVMetadataObject.ObjectType) -> CodeFormat? {
        switch type {
        case .qr: .qr
        case .aztec: .aztec
        case .dataMatrix: .dataMatrix
        case .pdf417: .pdf417
        case .ean13: .ean13
        case .ean8: .ean8
        case .upce: .upcE
        case .code128: .code128
        case .code39,
             .code39Mod43: .code39
        case .code93: .code93
        case .itf14,
             .interleaved2of5: .itf
        case .codabar: .codabar
        default: nil
        }
    }

    /// Exhaustive, so a format added to the protocol does not build until it is read.
    /// UPC-A has no type of its own: AVFoundation reads it as EAN-13 with a
    /// leading zero, which is how products expect it.
    private static func metadataTypes(for format: CodeFormat) -> [AVMetadataObject.ObjectType] {
        switch format {
        case .qr: [.qr]
        case .aztec: [.aztec]
        case .dataMatrix: [.dataMatrix]
        case .pdf417: [.pdf417]
        case .ean13: [.ean13]
        case .ean8: [.ean8]
        case .upcE: [.upce]
        case .code128: [.code128]
        case .code39: [.code39, .code39Mod43]
        case .code93: [.code93]
        case .itf: [.itf14, .interleaved2of5]
        case .codabar: [.codabar]
        }
    }
}
