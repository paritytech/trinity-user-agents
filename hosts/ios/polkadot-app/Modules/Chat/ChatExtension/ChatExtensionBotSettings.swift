import Keystore_iOS

protocol ChatExtensionBotSettings {
    func isEnabled(extId: ChatExtension.Id) -> Bool
    func set(enabled: Bool, for extId: ChatExtension.Id)
    func hasActionResponse(
        from ext: ChatExtending,
        for action: ChatExtension.ActionId
    ) -> Bool
    func mark(action: ChatExtension.ActionId, for ext: ChatExtending)
}

extension ChatExtensionBotSettings where Self: SettingsManagerProtocol {
    private static var actionKey: String { "action" }
    private static var enabledKey: String { "enabled" }

    private func key(
        from extId: ChatExtension.Id
    ) -> String {
        "extension:" + (extId.isEmpty ? "unknown" : extId)
    }

    func isEnabled(extId: ChatExtension.Id) -> Bool {
        guard let extensionInfo = anyValue(for: key(from: extId)) as? [String: Any] else {
            return false
        }
        return extensionInfo[Self.enabledKey] as? Bool ?? false
    }

    func set(enabled: Bool, for extId: ChatExtension.Id) {
        var extensionInfo = (anyValue(for: key(from: extId)) as? [String: Any]) ?? [:]
        extensionInfo[Self.enabledKey] = enabled

        set(anyValue: extensionInfo, for: key(from: extId))
    }

    func hasActionResponse(
        from ext: ChatExtending,
        for action: ChatExtension.ActionId
    ) -> Bool {
        guard let extensionInfo = anyValue(for: key(from: ext.identifier)) as? [String: Any] else {
            return false
        }

        if let actions = extensionInfo[Self.actionKey] as? [String] {
            return actions.contains(action)
        }
        return false
    }

    func mark(action: ChatExtension.ActionId, for ext: ChatExtending) {
        guard !hasActionResponse(from: ext, for: action) else {
            return
        }

        var extensionInfo = (anyValue(for: key(from: ext.identifier)) as? [String: Any]) ?? [:]
        var actions = (extensionInfo[Self.actionKey] as? [String]) ?? []
        actions.append(action)
        extensionInfo[Self.actionKey] = actions

        set(anyValue: extensionInfo, for: key(from: ext.identifier))
    }
}

extension SettingsManager: ChatExtensionBotSettings {}
