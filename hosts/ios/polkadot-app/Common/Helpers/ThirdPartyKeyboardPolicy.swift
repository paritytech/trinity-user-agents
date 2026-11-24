import UIKit

/// Refuses third-party keyboards while a restricted control is being edited.
/// Read by `AppDelegate` in `application(_:shouldAllowExtensionPointIdentifier:)`, which runs
/// before `editingDidBegin`, so the restriction starts at `editingWillBegin`.
@MainActor
enum ThirdPartyKeyboardPolicy {
    private weak static var editingControl: UIControl?

    static var isRestricted: Bool {
        editingControl != nil
    }

    static func restrict(_ control: UIControl) {
        control.addAction(UIAction { [weak control] _ in
            editingControl = control
        }, for: .editingWillBegin)

        control.addAction(UIAction { [weak control] _ in
            guard editingControl === control else {
                return
            }

            editingControl = nil
        }, for: .editingDidEnd)
    }
}
