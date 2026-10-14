import SwiftUI
import Products

// TODO: Temp interface for adding products: will be removed in future
struct DebugProductEditView: View {
    @State var name: String
    @State var scriptURL: String

    let title: String
    let onSave: (String, String) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Product Info") {
                    TextField("Name", text: $name)
                    TextField("Script URL", text: $scriptURL)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }
            }
            .navigationTitle(title)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        onSave(name, scriptURL)
                        dismiss()
                    }
                    .disabled(name.isEmpty || scriptURL.isEmpty)
                }
            }
        }
    }
}
