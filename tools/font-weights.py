"""Freeze the bundled variable Inter font at the web UI's weights/optical sizes."""
import sys
import json
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools import subset

directory = Path(sys.argv[1])
font = TTFont(directory / "font.ttf")
axes = {axis.axisTag for axis in font["fvar"].axes} if "fvar" in font else set()
metrics = {}
for weight in (350, 400, 450, 500, 550, 600, 650, 800):
    for display in (False, True):
        values = {"wght": weight} if "wght" in axes else {}
        if "opsz" in axes:
            values["opsz"] = 32 if display else 14
        instance = instantiateVariableFont(font, values, inplace=False) if axes else font
        instance.save(directory / f"font-{'display-' if display else ''}{weight}.ttf")
        units = instance["head"].unitsPerEm
        advances = instance["hmtx"].metrics
        metrics[f"{'display-' if display else ''}{weight}"] = {
            chr(codepoint): advances[glyph][0] / units
            for codepoint, glyph in instance.getBestCmap().items()
        }
def is_hangul(codepoint):
    # Keep in sync with is_hangul() in src/renderer.rs, which routes these to font-kr.
    return (0x1100 <= codepoint <= 0x11FF or 0x3130 <= codepoint <= 0x318F
            or 0xA960 <= codepoint <= 0xA97F or 0xAC00 <= codepoint <= 0xD7A3
            or 0xD7B0 <= codepoint <= 0xD7FF)


def fallback(path, keep, family, prefix):
    font = TTFont(path)
    options = subset.Options()
    options.layout_features = []
    options.name_IDs = ["*"]
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=[codepoint for codepoint in font.getBestCmap() if keep(codepoint)])
    subsetter.subset(font)
    advances_by_weight = {}
    for weight in (400, 500):
        instance = instantiateVariableFont(font, {"wght": weight}, inplace=False)
        # Derived subsets use their own family name while retaining copyright/license tables.
        style = "Regular" if weight == 400 else "Medium"
        for name_id, value in [(1, family), (2, style), (4, f"{family} {style}"),
                               (6, f"{family.replace(' ', '')}-{style}"), (16, family), (17, style)]:
            instance["name"].setName(value, name_id, 3, 1, 0x409)
        instance.save(directory / f"{prefix}-{weight}.ttf")
        units = instance["head"].unitsPerEm
        advances = instance["hmtx"].metrics
        advances_by_weight[weight] = {chr(cp): advances[glyph][0] / units
                                      for cp, glyph in instance.getBestCmap().items()}
    return advances_by_weight


cjk = fallback(sys.argv[2],
               lambda cp: cp < 0xA000 or 0xF900 <= cp <= 0xFAFF or 0xFF00 <= cp <= 0xFFEF,
               "Luma CJK", "font-cjk")
# Noto Sans SC carries no Hangul syllables; Korean comes from Noto Sans KR.
hangul = fallback(sys.argv[3], is_hangul, "Luma KR", "font-kr")
for weight in (400, 500):
    metrics[f"fallback-{weight}"] = {**cjk[weight], **hangul[weight]}
(directory / "font-metrics.json").write_text(json.dumps(metrics, ensure_ascii=False), encoding="utf8")
