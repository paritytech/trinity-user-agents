import Foundation

/// Device capabilities a product can request access to.
/// Raw values match JS bridge
/// contract (capability: String) decodes directly on both platforms.
public enum DeviceCapabilityType: String, CaseIterable, Sendable {
    case notifications = "Notifications"
    case camera = "Camera"
    case microphone = "Microphone"
    case bluetooth = "Bluetooth"
    case nfc = "NFC"
    case location = "Location"
    case clipboard = "Clipboard"
    case openUrl = "OpenUrl"
    case biometrics = "Biometrics"
}
