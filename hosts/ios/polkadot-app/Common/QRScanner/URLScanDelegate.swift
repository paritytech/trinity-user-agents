import Foundation
import SubstrateSdk

@MainActor
protocol URLScanDelegate: AnyObject {
    func urlScanDidReceiveResult(_ url: URL)
}
