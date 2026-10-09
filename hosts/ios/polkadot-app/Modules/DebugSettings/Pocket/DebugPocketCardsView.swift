import SwiftUI

/// Cards typed in by hand, for driving a product's Pocket before it
/// publishes a manifest.
///
/// A card named here is only offered for a product with **no published
/// worker**: a published one always wins. Its widget URL is the exception and
/// opens even over a published widget.
struct DebugPocketCardsView: View {
    @State var viewModel = DebugPocketCardsViewModel()

    var body: some View {
        List {
            Section {
                ForEach(viewModel.cards) { card in
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\(card.title): \(card.cardId)")
                            .font(.headline)
                        Text(card.productId)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(card.faceUrl)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        if let widgetUrl = card.widgetUrl {
                            Text(widgetUrl)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        if card.faceShown == false {
                            Text("Opens with the face away")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .onDelete { viewModel.delete(at: $0) }
            }

            Section("Add a card") {
                TextField("Product (dotNS name)", text: $viewModel.productId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Card id", text: $viewModel.cardId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Title", text: $viewModel.title)
                TextField("Face URL", text: $viewModel.faceUrl)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Widget URL (optional)", text: $viewModel.widgetUrl)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Toggle("Open with the face away", isOn: $viewModel.opensWithFaceAway)

                Button("Save") { viewModel.save() }
                    .disabled(!viewModel.canSave)

                if let refusal = viewModel.refusal {
                    Text(refusal).foregroundStyle(.red)
                }
            }

            Section("Open") {
                ForEach(viewModel.cards) { card in
                    Text("polkadot://\(card.productId)/-/pocket/add?card=\(card.cardId)")
                        .font(.caption)
                        .textSelection(.enabled)
                }
            }
        }
        .navigationTitle("Pocket cards")
        .task { viewModel.load() }
    }
}
