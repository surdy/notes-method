# Notesmith App Icon Concepts — Round 3 (Opus 4.7)

Five flat-color icon concepts exploring origami, tangram, river deltas, Penrose tiles, and sediment layers. All 512×512, no gradients, no forbidden motifs.

---

## 1. Folded Note (Origami)

**Description:** A single sheet of paper caught mid-fold, with one corner turned over to reveal an inner color. Reads as both a stylized "N" and a dog-eared note.

**Colors:** `#1E2A3A` background · `#F5F1E8` paper · `#E8893B` accent fold

**Symbolism:** The deliberate craft of folding raw thought into shape — every note is a sheet shaped by hand.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="116" fill="#1E2A3A"/>
  <path d="M 128 96 L 384 96 L 384 416 L 192 416 L 128 352 Z" fill="#F5F1E8"/>
  <path d="M 128 352 L 192 352 L 192 416 Z" fill="#E8893B"/>
  <path d="M 384 96 L 256 224 L 384 224 Z" fill="#E8893B"/>
  <path d="M 256 224 L 384 96 L 384 224 Z" fill="#C46B1E" opacity="0.5"/>
</svg>
```

---

## 2. Tangram N

**Description:** Seven classic tangram pieces rearranged into a chunky letter N. Geometric, balanced, instantly legible as a wordmark.

**Colors:** `#0F1419` background · `#7DD3C0` teal pieces · `#F4A261` warm pieces

**Symbolism:** A finite set of pieces compose infinite shapes — the same primitives (notes) build any structure you need.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="116" fill="#0F1419"/>
  <polygon points="112,112 208,112 208,208" fill="#7DD3C0"/>
  <polygon points="112,112 208,208 112,304" fill="#F4A261"/>
  <polygon points="112,304 112,400 208,400 208,304" fill="#7DD3C0"/>
  <polygon points="208,112 400,400 400,304 208,208" fill="#F4A261"/>
  <polygon points="304,112 400,112 400,208" fill="#7DD3C0"/>
  <polygon points="304,112 208,112 208,208" fill="#F4A261" opacity="0.6"/>
  <polygon points="304,400 400,400 400,304" fill="#7DD3C0" opacity="0.6"/>
</svg>
```

---

## 3. Delta

**Description:** A trunk channel branching into a fan of distributaries flowing toward the bottom edge. Stylized like a topographic flow diagram, but composed of solid wedges, not contour lines.

**Colors:** `#142028` background · `#3DA5D9` water · `#E8C547` silt bank

**Symbolism:** Notes flow downstream from a single source, fanning into the many places they need to be — capture once, distribute everywhere.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="116" fill="#142028"/>
  <polygon points="0,512 512,512 416,320 96,320" fill="#E8C547"/>
  <polygon points="240,80 272,80 272,320 240,320" fill="#3DA5D9"/>
  <polygon points="256,288 144,512 192,512 272,320" fill="#3DA5D9"/>
  <polygon points="256,288 240,320 320,512 368,512" fill="#3DA5D9"/>
  <polygon points="256,304 256,512 280,512" fill="#3DA5D9"/>
  <polygon points="256,288 80,512 128,512 256,336" fill="#3DA5D9"/>
  <polygon points="256,288 432,512 384,512 256,336" fill="#3DA5D9"/>
</svg>
```

---

## 4. Penrose Wedge

**Description:** Three Penrose-style kite tiles meeting at a single vertex with five-fold symmetry suggested by their angles. A non-repeating pattern frozen at its seed point.

**Colors:** `#1A1626` background · `#C77DFF` violet kite · `#7B2CBF` deep plum kite

**Symbolism:** Aperiodic tiles never repeat yet always fit — like a vault of notes that connect without ever collapsing into sameness.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="116" fill="#1A1626"/>
  <polygon points="256,256 144,144 256,80 368,144" fill="#C77DFF"/>
  <polygon points="256,256 368,144 432,256 368,368" fill="#7B2CBF"/>
  <polygon points="256,256 368,368 256,432 144,368" fill="#C77DFF"/>
  <polygon points="256,256 144,368 80,256 144,144" fill="#7B2CBF"/>
  <polygon points="256,256 256,80 368,144" fill="#9D4EDD"/>
  <polygon points="256,256 432,256 368,368" fill="#9D4EDD" opacity="0.7"/>
</svg>
```

---

## 5. Sediment

**Description:** Horizontal strata of warm earth tones stacked from the bottom, with one ribbon of cooler stone cutting across like a marker bed. The top layer is thinnest — the freshest deposit.

**Colors:** `#0F0E0C` background · `#D4A574` sandstone · `#5C7A89` marker bed

**Symbolism:** Knowledge accumulates in layers — old notes form the bedrock, new notes settle on top, and a single insight cuts cleanly across them all.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="116" fill="#0F0E0C"/>
  <rect x="80" y="384" width="352" height="56" fill="#8B5A2B"/>
  <rect x="80" y="320" width="352" height="64" fill="#A67343"/>
  <rect x="80" y="256" width="352" height="64" fill="#D4A574"/>
  <rect x="80" y="208" width="352" height="48" fill="#5C7A89"/>
  <rect x="80" y="144" width="352" height="64" fill="#C49060"/>
  <rect x="80" y="96" width="352" height="48" fill="#E5BC85"/>
</svg>
```
