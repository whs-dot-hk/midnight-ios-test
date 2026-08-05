import SwiftUI

struct ContentView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        TabView {
            NetworksView(viewModel: viewModel)
                .tabItem { Label("Networks", systemImage: "server.rack") }

            SettingsView(viewModel: viewModel)
                .tabItem { Label("Settings", systemImage: "gear") }
        }
        .task { viewModel.start() }
    }
}

struct NetworksView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        NavigationStack {
            List {
                ForEach(viewModel.monitors) { monitor in
                    Section {
                        HStack {
                            Circle()
                                .fill(color(for: monitor.status))
                                .frame(width: 10, height: 10)
                            Text(label(for: monitor.status))
                                .font(.subheadline)
                        }
                        if let summary = monitor.snapshot.summary {
                            LabeledContent("Best block", value: "#\(summary.bestBlock)")
                            LabeledContent("Finalized", value: "#\(summary.finalizedBlock)")
                            LabeledContent(
                                "Last block",
                                value: String(format: "%.0fs ago", summary.secondsSinceLastBlock))
                        }
                        ForEach(monitor.snapshot.nodes, id: \.id) { node in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(node.name.isEmpty ? "unnamed" : node.name)
                                Text("\(node.peers) peers · block #\(node.bestBlock)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    } header: {
                        Text("\(monitor.label ?? "Midnight") · \(monitor.snapshot.nodes.count) validators")
                    }
                }

                if !viewModel.recentAlerts.isEmpty {
                    Section("Recent alerts") {
                        ForEach(viewModel.recentAlerts) { alert in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(title(for: alert))
                                Text(alert.event.body)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Midnight Telemetry")
        }
    }

    /// The history spans every monitored chain, so each entry names its own.
    private func title(for alert: DisplayAlert) -> String {
        guard let network = alert.networkLabel else { return alert.event.title }
        return "\(network): \(alert.event.title)"
    }

    private func label(for status: ConnectionStatus) -> String {
        switch status {
        case .connecting: return "Connecting…"
        case .live: return "Live"
        case .reconnecting: return "Reconnecting…"
        }
    }

    private func color(for status: ConnectionStatus) -> Color {
        switch status {
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
                    Toggle("Monitor all networks", isOn: $viewModel.monitorAllNetworks)
                } header: {
                    Text("Networks")
                } footer: {
                    Text(viewModel.monitorAllNetworks
                         ? "Every chain the feed carries is monitored on its own connection, and any of them can raise an alert."
                         : "Only mainnet is monitored.")
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
        let base = "Alert when no new block has arrived for this long, on any monitored network."
        let averages = viewModel.monitors.compactMap { $0.snapshot.summary?.avgBlockTimeMs }
        guard let avgMs = averages.first else { return base }
        return base + String(format: " Current average block time is %.1fs.", Double(avgMs) / 1000)
    }
}

#Preview {
    ContentView(viewModel: TelemetryViewModel())
}

#Preview("Settings") {
    SettingsView(viewModel: TelemetryViewModel())
}
