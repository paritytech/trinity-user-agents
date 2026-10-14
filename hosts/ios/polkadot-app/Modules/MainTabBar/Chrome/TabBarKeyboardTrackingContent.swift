/// Hosted panel content that owns a text input. The chrome drives the focus transition from
/// the keyboard notifications so input layout, panel anchor and height animate as one.
@MainActor
protocol TabBarKeyboardTrackingContent: AnyObject {
    var isKeyboardInputFocused: Bool { get }
    func setKeyboardInputFocused(_ focused: Bool)
}
