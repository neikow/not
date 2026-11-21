# Privacy

Last updated: 21 November 2026.

not is a tasks, notes, habits and workouts app for Android and macOS. It's built so that your data
stays yours: it lives on your devices, and only goes where you send it.

## What not collects

Nothing. not has no analytics, no ads, no tracking, and no account. The developer receives no
data from the app.

## Crash reports

If not crashes, the next launch asks whether to report it. Reporting opens a GitHub bug report in
your browser with where the app stopped (code locations, no tasks or notes), which you read
before sending. Nothing is sent otherwise, and there is no crash-reporting service.

## Where your data lives

- **On your devices.** Tasks, notes, habits, workouts and settings are stored in the app's private
  storage on each device.
- **On your sync server, if you pair one.** Sync is optional. When you pair a device with a
  server, your data is sent to that server over HTTPS and kept there (encrypted at rest when the
  server is set up with a key), so your other devices can fetch it. The server is one you (or someone you trust) run; the developer has
  no access to it.
- **In files you export.** Exports and backups go wherever you save them.

## Permissions

Each is asked for only when you use the feature, and each is optional:

| Permission | Used for |
| --- | --- |
| Notifications, alarms | Reminders for tasks and habits, at the time you set |
| Calendar (read only) | Showing your events beside tasks in Today and Upcoming. Events are read on the device and never stored or sent |
| Location, including in the background | Reminders when you arrive at or leave a place you chose. Your location is checked on the device and never stored or sent |
| Camera | Scanning a pairing QR code. No picture is kept |
| Internet | Syncing with your server, and opening links |

## Links out

About and Support open web pages (GitHub, Ko-fi) in your browser. Those sites have their own
privacy policies.

## Deleting your data

Uninstalling the app deletes its data from the device. Delete data from your sync server with the
server's own tools (`not-server revoke`, or by removing its database).

## Contact

Questions: open an issue at <https://github.com/neikow/not/issues>.
