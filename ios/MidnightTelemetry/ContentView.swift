import SwiftUI

struct ContentView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        NavigationStack {
            List {
                Section {
                    HStack {
                        Circle()
                            .fill(statusColor)
                            .frame(width: 10, height: 10)
                        Text(statusLabel)
                            .font(.subheadline)
                    }
                    if let summary = viewModel.snapshot.summary {
                        LabeledContent("Best block", value: "#\(summary.bestBlock)")
                        LabeledContent("Finalized", value: "#\(summary.finalizedBlock)")
                        LabeledContent("Last block", value: String(format: "%.0fs ago", summary.secondsSinceLastBlock))
                    }
                }

                Section("Nodes (\(viewModel.snapshot.nodes.count))") {
                    ForEach(viewModel.snapshot.nodes, id: \.id) { node in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text(node.name.isEmpty ? "unnamed" : node.name)
                                    .font(.body.weight(node.isValidator ? .semibold : .regular))
                                Spacer()
                                Text(node.kindLabel)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Text("\(node.peers) peers · block #\(node.bestBlock)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                if !viewModel.recentAlerts.isEmpty {
                    Section("Recent alerts") {
                        ForEach(viewModel.recentAlerts, id: \.id) { alert in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(alert.title)
                                Text(alert.body)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Midnight Telemetry")
        }
        .task { viewModel.start() }
    }

    private var statusLabel: String {
        switch viewModel.status {
        case .connecting: return "Connecting…"
        case .live: return "Live"
        case .reconnecting: return "Reconnecting…"
        }
    }

    private var statusColor: Color {
        switch viewModel.status {
        case .connecting: return .yellow
        case .live: return .green
        case .reconnecting: return .orange
        }
    }
}

#Preview {
    ContentView(viewModel: TelemetryViewModel())
}
