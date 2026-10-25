---
'lg-luma': minor
---

Open Luma at about 20 s into a cold boot on webOS 5 and 6, instead of after the Homebrew hook. `home.sh enable` registers a small root helper (`luma-early`, LS2 service `com.webos.service.lumahome`), which LG's service hub can start from `core-boot-done`, and a compositor key filter that asks it to mount Luma and then opens Home. The cold-boot hook skips Node when the helper already mounted Luma, and it still mounts Luma if the early start failed. `home.sh disable` removes the helper and the filter.
