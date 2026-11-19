import AVFoundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product names formats in the protocol's terms and the camera reads
/// AVFoundation's. A format lost either way is a code the product asked for
/// and never gets.
struct ProductScanFormatsTests {
    @Test("Every format a product can name is read, and reads back as itself")
    func everyFormatRoundTrips() {
        let formats: [CodeFormat] = [
            .qr, .aztec, .dataMatrix, .pdf417, .ean13, .ean8, .upcE, .code128, .code39, .code93, .itf, .codabar
        ]

        for format in formats {
            let types = ProductScanFormats.metadataTypes(for: [format])
            #expect(!types.isEmpty, "\(format) is never read")
            #expect(types.allSatisfy { ProductScanFormats.format(of: $0) == format })
        }
    }

    @Test("AVFoundation's variants read as the product's format, and other types as none")
    func variantsAndUnknownTypes() {
        #expect(ProductScanFormats.format(of: .code39Mod43) == .code39)
        #expect(ProductScanFormats.format(of: .itf14) == .itf)
        #expect(ProductScanFormats.format(of: .interleaved2of5) == .itf)
        #expect(ProductScanFormats.format(of: .microQR) == nil)
    }
}
