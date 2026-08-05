import UserNotifications

final class NotificationManager {
    static func requestAuthorization() {
        UNUserNotificationCenter.current().delegate = ForegroundPresenter.shared
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func post(alert: AlertEvent, genesis: String, networkLabel: String?) {
        let content = UNMutableNotificationContent()
        content.title = networkLabel.map { "\($0): \(alert.title)" } ?? alert.title
        content.body = alert.body
        content.sound = .default

        // alert.id names a condition within one chain ("block-stall"), so every
        // monitored network produces the same ids. Namespacing by genesis keeps
        // one network's notification from replacing another's, while still
        // letting a chain's own re-notification replace its previous one.
        let request = UNNotificationRequest(
            identifier: "\(genesis)/\(alert.id)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}

/// Without a delegate, UNUserNotificationCenter suppresses notifications
/// entirely while the app is in the foreground — which is exactly when
/// alerts are most likely to fire, since that's when someone is watching
/// the feed. Kept alive as a static singleton since the delegate is weak.
private final class ForegroundPresenter: NSObject, UNUserNotificationCenterDelegate {
    static let shared = ForegroundPresenter()

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound, .list])
    }
}
