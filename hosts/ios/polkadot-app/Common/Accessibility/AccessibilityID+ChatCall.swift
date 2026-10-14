import ExternalAccessibility

extension ChatCallViewModel {
    var statusAccessibilityId: any AccessibilityIdentifying {
        isIncomingRinging
            ? AccessibilityID.IncomingCall.statusLabel
            : AccessibilityID.InCall.statusLabel
    }

    var endCallAccessibilityId: any AccessibilityIdentifying {
        isIncomingRinging
            ? AccessibilityID.IncomingCall.declineButton
            : AccessibilityID.InCall.endButton
    }

    private var isIncomingRinging: Bool {
        isIncoming && callState == .ringing
    }
}
