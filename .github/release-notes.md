## SquirrelDisk 1.1

- **New Windows window chrome.** The native title bar is gone: the top toolbar now shares the sidebar's colour, carries the sidebar-toggle and new-scan buttons at its left and minimise / maximise / close at its right, and no longer shows the logo or app title. The empty toolbar area drags the window (double-click to maximise), and the window edges resize again, with Aero snap.
- **Updates come from this repository.** The in-app updater and the remote-scan agent bootstrap now download from `clawuv/DCX` releases instead of the upstream fork.

### Downloads
- **macOS:** `SquirrelDisk-macOS.dmg` (Apple silicon and Intel) — open it and drag SquirrelDisk to Applications.
- **Windows:** `SquirrelDisk-Windows.msi` (installer) or `squirreldisk-x86_64-pc-windows-msvc.exe` (portable).
- **Linux:** `SquirrelDisk-x86_64.AppImage` (recommended) or `SquirrelDisk-Linux-x86_64.tar.gz`.

### First launch
**macOS** — if macOS says SquirrelDisk can't be opened: open it once, then go to **System Settings → Privacy & Security** and click **Open Anyway**. If it says the app is damaged, run in Terminal:

```
xattr -dr com.apple.quarantine /Applications/SquirrelDisk.app
```

**Windows** — if you see "Windows protected your PC", click **More info → Run anyway**. For the portable `.exe` you can also right-click it → **Properties** → tick **Unblock**.

The `squirreldisk-agent-*` files are used automatically when scanning remote servers.
