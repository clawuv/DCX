## DCX 1.1

- **New Windows window chrome.** The native title bar is gone: the top toolbar now shares the sidebar's colour, carries the sidebar-toggle and new-scan buttons at its left and minimise / maximise / close at its right, and no longer shows the logo or app title. The empty toolbar area drags the window (double-click to maximise), and the window edges resize again, with Aero snap.
- **Updates come from this repository.** The in-app updater and the remote-scan agent bootstrap now download from `clawuv/DCX` releases instead of the upstream fork.

### Downloads
- **macOS:** `DCX-macOS.dmg` (Apple silicon and Intel) — open it and drag DCX to Applications.
- **Windows:** `DCX-Windows.msi` (installer) or `dcx-x86_64-pc-windows-msvc.exe` (portable).
- **Linux:** `DCX-x86_64.AppImage` (recommended) or `DCX-Linux-x86_64.tar.gz`.

### First launch
**macOS** — if macOS says DCX can't be opened: open it once, then go to **System Settings → Privacy & Security** and click **Open Anyway**. If it says the app is damaged, run in Terminal:

```
xattr -dr com.apple.quarantine /Applications/DCX.app
```

**Windows** — if you see "Windows protected your PC", click **More info → Run anyway**. For the portable `.exe` you can also right-click it → **Properties** → tick **Unblock**.

The `dcx-agent-*` files are used automatically when scanning remote servers.
