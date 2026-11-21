import Foundation

/// Crashes, reported only if the user says so: macOS writes a report for each crash
/// (~/Library/Logs/DiagnosticReports/not-….ips); on the next launch the app offers to open a
/// GitHub bug report with where it stopped, which the user reads before sending. No crash service.
/// Mirrors `CrashReports` on Android.
enum CrashReports {
    static let checkedKey = "crashesCheckedAt"

    /// A summary of the newest crash since the last launch, or nil. The first launch only starts
    /// the clock, so crashes from before aren't brought up.
    static func pending() -> String? {
        let defaults = UserDefaults.standard
        let now = Date()
        defer { defaults.set(now, forKey: checkedKey) }
        guard let since = defaults.object(forKey: checkedKey) as? Date else { return nil }
        let folder = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Logs/DiagnosticReports")
        let files = (try? FileManager.default.contentsOfDirectory(
            at: folder, includingPropertiesForKeys: [.contentModificationDateKey]
        )) ?? []
        let newest = files
            .filter { $0.lastPathComponent.hasPrefix("not-") && $0.pathExtension == "ips" }
            .compactMap { url -> (URL, Date)? in
                guard let date = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                      date > since else { return nil }
                return (url, date)
            }
            .max { $0.1 < $1.1 }
        guard let (url, _) = newest, let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        return summary(ips: text)
    }

    /// What an .ips report says went wrong: the exception and the crashed thread's frames. The
    /// format is a JSON header line, then a JSON body.
    static func summary(ips: String) -> String? {
        guard let newline = ips.firstIndex(of: "\n"),
              let body = try? JSONSerialization.jsonObject(with: Data(ips[ips.index(after: newline)...].utf8)) as? [String: Any]
        else { return nil }
        var lines: [String] = []
        if let version = (body["bundleInfo"] as? [String: Any])?["CFBundleShortVersionString"] as? String {
            lines.append("not \(version) on \(AboutLinks.system)")
        }
        if let exception = body["exception"] as? [String: Any] {
            lines.append(["type", "signal", "subtype"].compactMap { exception[$0] as? String }.joined(separator: " "))
        }
        if let indicator = (body["termination"] as? [String: Any])?["indicator"] as? String {
            lines.append(indicator)
        }
        let images = (body["usedImages"] as? [[String: Any]] ?? []).map { $0["name"] as? String ?? "?" }
        if let threads = body["threads"] as? [[String: Any]],
           let faulting = body["faultingThread"] as? Int, faulting < threads.count,
           let frames = threads[faulting]["frames"] as? [[String: Any]] {
            lines.append("")
            for frame in frames.prefix(30) {
                let image = (frame["imageIndex"] as? Int).flatMap { $0 < images.count ? images[$0] : nil } ?? "?"
                let symbol = frame["symbol"] as? String ?? "+\(frame["imageOffset"] as? Int ?? 0)"
                lines.append("    \(image)  \(symbol)")
            }
        }
        return lines.isEmpty ? nil : lines.joined(separator: "\n")
    }
}
