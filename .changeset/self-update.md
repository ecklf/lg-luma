---
'lg-luma': minor
---

Update Luma from the TV: Settings → System → Luma update checks the latest GitHub release, then downloads the IPK, verifies its size and SHA-256 against the release's Homebrew Channel feed and installs it as root through Homebrew Channel. The installer runs in its own systemd unit so it outlives Luma being closed by the install, and remounts Luma as Home when the Home integration is enabled.
