# Input trademark artwork

These are the actual trademark vectors, not original Luma approximations. Used only
to identify the corresponding input device or media format. No endorsement is implied;
all trademarks remain the property of their respective owners.

| Input | Source / attribution |
| --- | --- |
| Sony | [Sony wordmark](https://commons.wikimedia.org/wiki/File:Sony_logo.svg), Sony / Yasuo Kuroki |
| DVD | [DVD logo](https://commons.wikimedia.org/wiki/File:DVD_logo.svg), DVD Forum, extracted from the official member PDF |
| Blu-ray | [Blu-ray Disc logo](https://commons.wikimedia.org/wiki/File:Blu-ray_Disc.svg), Blu-ray Disc Association, derived from the press EPS |

The linked Commons descriptions classify these text/geometric logos as **PD-textlogo**.
That copyright classification does not grant trademark rights or certify a device.
“Disk” uses an original generic disc icon, not trademark artwork.

`tools/input-trademark-assets.mjs` pins SHA-256 hashes of the original downloads.
It preserves the complete mark and its paths, centers it in a square viewport without
distortion, and resolves the Blu-ray SVG's namespace entities. The mesh compiler
renders the marks in monochrome for the existing input tint/focus treatment. No
artwork is fetched at runtime or during normal builds.
