---
'lg-luma': minor
---

Support `home.sh enable` on webOS 5 and 6. There, SAM cannot reread a mounted Home manifest, so the takeover bind-mounts Luma's executable over LG's Home executable instead, keeps LG's manifest and role, and relies on Luma's own lifecycle registration. Enabling, the cold-boot hook, status and restoring LG Home work as on webOS 25/26; a failed launch restores LG Home, package updates are remounted, and a normal tile launch opens the running Home instance.
