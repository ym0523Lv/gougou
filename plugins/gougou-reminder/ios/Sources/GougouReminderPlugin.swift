import SwiftRs
import Tauri
import UIKit
import UserNotifications
import WebKit

class ScheduleArgs: Decodable {
  let enabled: Bool
  let hour: Int
  let minute: Int
  let precise: Bool
  let quietWeekdays: [Int]
  let pausedUntil: String?
  let skipDates: [String]
  let title: String
  let body: String
}

class GougouReminderPlugin: Plugin, UNUserNotificationCenterDelegate {
  private let center = UNUserNotificationCenter.current()

  private func becomeNotificationDelegate() {
    center.delegate = self
  }

  private func resolveStatus(_ invoke: Invoke, effectivePrecise: Bool = false) {
    center.getNotificationSettings { settings in
      self.center.getPendingNotificationRequests { requests in
        let permission: String
        switch settings.authorizationStatus {
        case .authorized, .provisional, .ephemeral:
          permission = "granted"
        case .denied:
          permission = "denied"
        default:
          permission = "prompt"
        }
        invoke.resolve([
          "supported": true,
          "permission": permission,
          "exactAlarmAllowed": false,
          "effectivePrecise": effectivePrecise,
          "scheduledCount": requests.filter { $0.identifier.hasPrefix("gougou-reminder-") }.count,
          "backgroundSettingsAvailable": false,
        ])
      }
    }
  }

  @objc public func getStatus(_ invoke: Invoke) {
    becomeNotificationDelegate()
    resolveStatus(invoke)
  }

  @objc public func requestPermission(_ invoke: Invoke) {
    becomeNotificationDelegate()
    center.requestAuthorization(options: [.alert, .sound]) { _, _ in
      self.resolveStatus(invoke)
    }
  }

  @objc public func cancelAll(_ invoke: Invoke) {
    becomeNotificationDelegate()
    center.getPendingNotificationRequests { requests in
      let identifiers = requests.map(\.identifier).filter { $0.hasPrefix("gougou-reminder-") }
      self.center.removePendingNotificationRequests(withIdentifiers: identifiers)
      self.resolveStatus(invoke)
    }
  }

  @objc public func syncSchedule(_ invoke: Invoke) throws {
    becomeNotificationDelegate()
    let args = try invoke.parseArgs(ScheduleArgs.self)
    center.getPendingNotificationRequests { requests in
      let identifiers = requests.map(\.identifier).filter { $0.hasPrefix("gougou-reminder-") }
      self.center.removePendingNotificationRequests(withIdentifiers: identifiers)
      guard args.enabled else {
        self.resolveStatus(invoke)
        return
      }

      let calendar = Calendar.current
      let formatter = DateFormatter()
      formatter.calendar = calendar
      formatter.locale = Locale(identifier: "en_US_POSIX")
      formatter.dateFormat = "yyyy-MM-dd"
      let start = calendar.startOfDay(for: Date())
      var scheduled = 0
      let group = DispatchGroup()
      for offset in 0..<45 where scheduled < 30 {
        guard let day = calendar.date(byAdding: .day, value: offset, to: start) else { continue }
        let date = formatter.string(from: day)
        let weekday = calendar.component(.weekday, from: day)
        let isoWeekday = weekday == 1 ? 7 : weekday - 1
        if args.quietWeekdays.contains(isoWeekday) ||
          args.skipDates.contains(date) ||
          (args.pausedUntil != nil && date <= args.pausedUntil!) {
          continue
        }
        var components = calendar.dateComponents([.year, .month, .day], from: day)
        components.hour = args.hour
        components.minute = args.minute
        guard let triggerDate = calendar.date(from: components), triggerDate > Date() else { continue }

        let content = UNMutableNotificationContent()
        content.title = args.title
        content.body = args.body
        content.sound = .default
        content.userInfo = ["gougouTargetDate": date]
        let trigger = UNCalendarNotificationTrigger(dateMatching: components, repeats: false)
        let request = UNNotificationRequest(
          identifier: "gougou-reminder-\(date)",
          content: content,
          trigger: trigger
        )
        group.enter()
        self.center.add(request) { _ in group.leave() }
        scheduled += 1
      }
      group.notify(queue: .main) { self.resolveStatus(invoke) }
    }
  }

  @objc public func takeNotificationTarget(_ invoke: Invoke) {
    let defaults = UserDefaults.standard
    let date = defaults.string(forKey: "gougouNotificationTarget")
    defaults.removeObject(forKey: "gougouNotificationTarget")
    invoke.resolve(["targetDate": date as Any])
  }

  @objc public func openBackgroundSettings(_ invoke: Invoke) {
    resolveStatus(invoke)
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    if let date = response.notification.request.content.userInfo["gougouTargetDate"] as? String {
      UserDefaults.standard.set(date, forKey: "gougouNotificationTarget")
    }
    completionHandler()
  }
}

@_cdecl("init_plugin_gougou_reminder")
func initPlugin() -> Plugin {
  return GougouReminderPlugin()
}
