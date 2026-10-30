concord @VERSION@ for Mac — Apple Silicon, macOS 13+
==================================================

FIRST OPEN
----------
Drag concord.app into Applications. The app is ad-hoc signed, not notarized; only
bypass macOS warnings if you trust the download. Right-click > Open and confirm,
or use System Settings > Privacy & Security > Open Anyway after a blocked launch.

PERMISSIONS
-----------
- Microphone: voice rooms and calls.
- Camera: when you enable your camera.
- Screen Recording: screen/window sharing and app audio. Quit and reopen after granting.
- Local Network: servers on your LAN; denied access can look like an unresponsive server.
- Keychain: saved passwords. macOS may request access on first use or after updates;
  choose Always Allow if you trust the app. Configuration stores references, not passwords.

Change denied permissions in System Settings > Privacy & Security.

MAC LIMITATIONS
---------------
- No global push-to-talk; use the microphone button or voice detection.
- No in-app hosting; join another server.
- No copying images to the clipboard.
- No menu bar icon or macOS notifications. Closing the window quits the app.

Logs: ~/Library/Logs/concord
Settings: ~/Library/Application Support/concord
Security: see SECURITY.md in the repository.
