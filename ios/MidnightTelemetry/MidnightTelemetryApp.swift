import SwiftUI

@main
struct MidnightTelemetryApp: App {
    @StateObject private var viewModel = TelemetryViewModel()

    init() {
        NotificationManager.requestAuthorization()
    }

    var body: some Scene {
        WindowGroup {
            ContentView(viewModel: viewModel)
        }
    }
}
