import SwiftUI

struct ContentView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        TabView {
            NodesView(viewModel: viewModel)
                .tabItem { Label("Nodes", systemImage: "server.rack") }

            SettingsView(viewModel: viewModel)
                .tabItem { Label("Settings", systemImage: "gear") }
        }
        .task { viewModel.start() }
    }
}

struct NodesView: View {
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
                        ForEach(viewModel.recentAlerts) { alert in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(alert.event.title)
                                Text(alert.event.body)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle(viewModel.currentChainLabel ?? "Midnight Telemetry")
        }
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

struct SettingsView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if viewModel.snapshot.chains.isEmpty {
                        // The list is announced by the feed on connect, so it is
                        // briefly unavailable before the first message arrives.
                        LabeledContent("Network", value: "Waiting for feed…")
                    } else {
                        Picker("Network", selection: $viewModel.genesis) {
                            ForEach(viewModel.snapshot.chains, id: \.genesis) { chain in
                                Text("\(chain.label) (\(chain.nodeCount))")
                                    .tag(chain.genesis)
                            }
                        }
                    }
                } header: {
                    Text("Network")
                } footer: {
                    Text("Chains and node counts are reported by the telemetry feed.")
                }

                Section {
                    Stepper(value: $viewModel.blockStallSecs, in: 7...120, step: 1) {
                        LabeledContent(
                            "Stall threshold",
                            value: String(format: "%.0fs", viewModel.blockStallSecs)
                        )
                    }
                } header: {
                    Text("Block production")
                } footer: {
                    Text(thresholdFooter)
                }
            }
            .navigationTitle("Settings")
        }
    }

    /// Midnight blocks land ~every 6s, so a threshold close to that fires on
    /// normal jitter rather than on real stalls — show the live average so the
    /// value can be picked against what the chain is actually doing.
    private var thresholdFooter: String {
        let base = "Alert when no new block has arrived for this long."
        guard let avgMs = viewModel.snapshot.summary?.avgBlockTimeMs else { return base }
        return base + String(format: " Current average block time is %.1fs.", Double(avgMs) / 1000)
    }
}

#Preview {
    ContentView(viewModel: TelemetryViewModel())
}

#Preview("Settings") {
    SettingsView(viewModel: TelemetryViewModel())
}
