import SwiftUI

struct ContentView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        TabView {
            ValidatorsView(viewModel: viewModel)
                .tabItem { Label("Validators", systemImage: "checkmark.seal") }

            SettingsView(viewModel: viewModel)
                .tabItem { Label("Settings", systemImage: "gear") }
        }
        .task { viewModel.start() }
    }
}

struct ValidatorsView: View {
    @ObservedObject var viewModel: TelemetryViewModel

    var body: some View {
        NavigationStack {
            List {
                ForEach(viewModel.monitors) { monitor in
                    Section {
                        ChainStats(monitor: monitor)
                        ForEach(monitor.snapshot.nodes, id: \.id) { node in
                            ValidatorRow(node: node, chainTip: monitor.snapshot.summary?.bestBlock)
                        }
                    } header: {
                        NetworkHeader(monitor: monitor)
                    }
                }

                if !viewModel.recentAlerts.isEmpty {
                    Section("Recent alerts") {
                        ForEach(viewModel.recentAlerts) { alert in
                            AlertRow(alert: alert)
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Validators")
        }
    }
}

/// Network name with its connection state, and how many validators it reports.
private struct NetworkHeader: View {
    let monitor: NetworkMonitor

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)
            Text(monitor.label ?? "Midnight")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
            Spacer()
            Text(validatorCount)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        // Section headers are upper-cased by default, which mangles the feed's
        // own chain names.
        .textCase(nil)
    }

    private var validatorCount: String {
        let count = monitor.snapshot.nodes.count
        return count == 1 ? "1 validator" : "\(count) validators"
    }

    private var statusColor: Color {
        switch monitor.status {
        case .connecting: return .yellow
        case .live: return .green
        case .reconnecting: return .orange
        }
    }
}

/// Chain-level figures, side by side rather than as three separate rows.
private struct ChainStats: View {
    let monitor: NetworkMonitor

    var body: some View {
        HStack(spacing: 0) {
            metric("Best", summary.map { "#\($0.bestBlock)" })
            divider
            metric("Finalized", summary.map { "#\($0.finalizedBlock)" })
            divider
            metric("Last block", summary.map { String(format: "%.0fs ago", $0.secondsSinceLastBlock) })
        }
    }

    private var summary: NetworkSummary? { monitor.snapshot.summary }

    private var divider: some View {
        Divider().frame(height: 26)
    }

    private func metric(_ title: String, _ value: String?) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            // Monospaced digits so live-updating numbers don't shift width.
            Text(value ?? "—")
                .font(.footnote.monospacedDigit())
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct ValidatorRow: View {
    let node: NodeInfo
    let chainTip: UInt64?

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(node.name.isEmpty ? "unnamed" : node.name)
                    .font(.callout)
                Text(node.peers == 1 ? "1 peer" : "\(node.peers) peers")
                    .font(.caption)
                    .foregroundStyle(node.peers == 0 ? .red : .secondary)
            }
            Spacer(minLength: 0)
            VStack(alignment: .trailing, spacing: 2) {
                Text("#\(node.bestBlock)")
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(blocksBehind == nil ? .primary : .orange)
                if let behind = blocksBehind {
                    Text("\(behind) behind")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            }
        }
    }

    /// A validator trailing the chain tip is the signal worth surfacing. A block
    /// or two is ordinary propagation delay, so only a real gap is flagged.
    private var blocksBehind: Int64? {
        guard let tip = chainTip, tip > 0 else { return nil }
        let lag = Int64(tip) - Int64(node.bestBlock)
        return lag > 2 ? lag : nil
    }
}

private struct AlertRow: View {
    let alert: DisplayAlert

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                // The history spans every monitored chain, so each entry names its own.
                Text(alert.networkLabel.map { "\($0): \(alert.event.title)" } ?? alert.event.title)
                    .font(.callout)
                Text(alert.event.body)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var icon: String {
        alert.event.resolved ? "checkmark.circle.fill" : "exclamationmark.triangle.fill"
    }

    private var tint: Color {
        if alert.event.resolved { return .green }
        return alert.event.severity == .critical ? .red : .orange
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
