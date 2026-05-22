# Notesmith — Icon Concepts (Round 2, Claude Opus 4.7 xHigh)

Five concepts, each from a deliberately different design school. None lean on the literal "smith / forge / anvil / quill" vocabulary that dominated round one. Every mark is flat — no gradients, no inner shadows, no faux 3D. Each SVG is production-ready at any size from 16×16 to 1024×1024.

---

## 1. Asterism

### Description
A six-node constellation rendered as bright dots connected by thin straight lines on a deep ink-navy field. The shape is an abstract asterism: a top apex, two shoulders, a slightly enlarged central anchor node, and two feet — read together as a small navigable graph. The lines are hairline thin so the dots dominate the silhouette; the anchor node carries an inner highlight to give it visual weight as the "current note" in a graph traversal.

### Color Palette
| Role | Hex |
|---|---|
| Field (background tile) | `#0F172A` (Ink Navy) |
| Star nodes | `#F59E0B` (Stellar Amber) |
| Anchor highlight | `#FEF3C7` (Lit Cream) |
| Connection lines | `#475569` (Slate Hairline) |

### Symbolism
A constellation is the original knowledge graph: arbitrary points made meaningful by the lines drawn between them. It captures Notesmith's core promise — that loose notes become a navigable structure once you connect them — without leaning on books, files, or folders. The amber-on-navy reads as a star chart and as a debugger's node graph at the same time.

### Small Size Strategy
At 32×32 the lines drop to ~1px and the six dots remain crisp, reading as a recognizable asterism. At 16×16 the inner highlight is removed; the silhouette becomes six amber pixels arranged in a pentagonal pattern with a center pip — still distinctly a "small constellation," not a generic icon blob.

### SVG
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="112" fill="#0F172A"/>
  <g stroke="#475569" stroke-width="6" stroke-linecap="round">
    <line x1="256" y1="112" x2="128" y2="240"/>
    <line x1="256" y1="112" x2="384" y2="240"/>
    <line x1="128" y1="240" x2="256" y2="320"/>
    <line x1="384" y1="240" x2="256" y2="320"/>
    <line x1="128" y1="240" x2="176" y2="416"/>
    <line x1="384" y1="240" x2="336" y2="416"/>
    <line x1="256" y1="320" x2="176" y2="416"/>
    <line x1="256" y1="320" x2="336" y2="416"/>
  </g>
  <g fill="#F59E0B">
    <circle cx="256" cy="112" r="22"/>
    <circle cx="128" cy="240" r="22"/>
    <circle cx="384" cy="240" r="22"/>
    <circle cx="256" cy="320" r="30"/>
    <circle cx="176" cy="416" r="18"/>
    <circle cx="336" cy="416" r="18"/>
  </g>
  <circle cx="256" cy="320" r="11" fill="#FEF3C7"/>
</svg>
```

---

## 2. Sigil VI

### Description
A bold ink-stamp sigil on a bone-white tile. A thick-stroked hexagon outline contains a Y-shaped trivium of three radii joining at the geometric center, dividing the hex into three equal trapezoidal chambers. A solid vermillion disc sits at the convergence point. The whole mark is rendered with miter joins and uniform stroke weight — no curves, no decoration, no flourishes. It feels like a maker's mark pressed into wax.

### Color Palette
| Role | Hex |
|---|---|
| Tile | `#FAFAF9` (Bone) |
| Ink (frame + radii) | `#18181B` (Iron Black) |
| Centerpoint | `#DC2626` (Vermillion) |

### Symbolism
The sigil tradition — medieval manuscripts, guild marks, occult glyphs — is the historical ancestor of dense personal knowledge systems. A hex divided into three chambers reads as: capture, organize, retrieve; or vault, route, link. The red dot at the convergence is the unifying point — the index, the present moment, the cursor. Recognizable as a *seal* rather than an *illustration*; it photocopies, embroiders, engraves, and stencils equally well.

### Small Size Strategy
The hex outline carries the silhouette down to 16×16 — a thick-bordered hexagon with a red pixel at center remains unmistakably this mark. The Y-radii thin progressively but never disappear because they share the centerpoint. At 32×32 all three chambers are clearly visible; at 16×16 the eye reads "hexagon + red pip" which is enough.

### SVG
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="112" fill="#FAFAF9"/>
  <polygon points="256,72 426,170 426,342 256,440 86,342 86,170"
           fill="none" stroke="#18181B" stroke-width="36" stroke-linejoin="miter"/>
  <g stroke="#18181B" stroke-width="36" stroke-linecap="butt">
    <line x1="256" y1="256" x2="256" y2="72"/>
    <line x1="256" y1="256" x2="86" y2="342"/>
    <line x1="256" y1="256" x2="426" y2="342"/>
  </g>
  <circle cx="256" cy="256" r="42" fill="#DC2626"/>
</svg>
```

---

## 3. Cartograph

### Description
Six concentric rounded-rectangle contour lines, decreasing in size and corner-radius toward the center, rendered in a single mint-green hairline weight on a deep forest field. A small gold pip sits slightly off-center, marking the "summit" — a survey monument on the highest contour. There are no fills, only line work; the entire icon reads as a flat topographic map of an imaginary plateau.

### Color Palette
| Role | Hex |
|---|---|
| Field | `#022C22` (Forest Ink) |
| Contour lines | `#34D399` (Mint Green) |
| Summit marker | `#FBBF24` (Survey Gold) |

### Symbolism
A topographic map is a flat representation of accumulated depth. That's exactly what a personal note vault is: every link, every revision, every backlink adds elevation to a flat surface of markdown files. The cartographic vocabulary also signals "professional tool, terse documentation, precise units" — appropriate for the technical-professional audience. The off-center summit hints that knowledge growth is asymmetric; the highest point is rarely where you expected it.

### Small Size Strategy
At 32×32 the outermost two contours hold the silhouette of a rounded rectangle; the inner contours collapse into perceived "texture." At 16×16 the icon collapses to a single rounded-rectangle outline with a gold dot — still cartographic, still distinct from a folder or document icon because the proportions (wider than tall, with a contained marker) are unusual.

### SVG
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="112" fill="#022C22"/>
  <g fill="none" stroke="#34D399" stroke-width="6">
    <rect x="64" y="144" width="384" height="304" rx="64"/>
    <rect x="96" y="168" width="320" height="264" rx="52"/>
    <rect x="128" y="192" width="256" height="224" rx="44"/>
    <rect x="160" y="216" width="192" height="184" rx="36"/>
    <rect x="192" y="240" width="128" height="144" rx="28"/>
    <rect x="224" y="264" width="64" height="104" rx="20"/>
  </g>
  <circle cx="272" cy="316" r="12" fill="#FBBF24"/>
</svg>
```

---

## 4. Aperture

### Description
Six trapezoidal blades fan around a hexagonal central opening, like the iris of a precision instrument. The blades are a single flat teal — the "fan effect" comes entirely from the way each blade's straight cutting edge is offset 60° from its outer-vertex pair, producing the characteristic slanted overlap of a mechanical aperture. The center opening is filled with a single saturated pink disc, signalling "the focused subject." The whole mark sits on a tungsten-grey field and is bilaterally symmetric across the vertical axis.

### Color Palette
| Role | Hex |
|---|---|
| Field | `#1F2937` (Tungsten) |
| Iris blades | `#0E7490` (Titanium Teal) |
| Subject (center disc) | `#F472B6` (Hot Magenta) |
| Blade dividing edges | `#1F2937` (Tungsten — pulled from field for contrast) |

### Symbolism
An aperture is the canonical icon for **focus** — letting the right amount of light in to capture a sharp image. Notesmith's job is the same: filter the noise, route what matters, focus attention on the right note at the right time. The mechanical-instrument aesthetic positions the app as a *tool* not a *journal*, which fits the technical-professional audience. The pink center is deliberate: it's the warmest color in any UI palette, signalling the human moment at the heart of the precision machinery.

### Small Size Strategy
The hexagonal silhouette and central magenta disc carry to 16×16 cleanly — that combination is rare among app icons and stays distinctive. At 32×32 the blade-edge slant lines drop to 1px and read as subtle facets without muddying the form. The hex+disc structure means the icon never collapses to "a circle on a square."

### SVG
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="112" fill="#1F2937"/>
  <path fill="#0E7490" fill-rule="evenodd"
        d="M 256 64 L 422 160 L 422 352 L 256 448 L 90 352 L 90 160 Z
           M 256 176 L 187 216 L 187 296 L 256 336 L 325 296 L 325 216 Z"/>
  <g stroke="#1F2937" stroke-width="8" stroke-linecap="round">
    <line x1="256" y1="176" x2="422" y2="160"/>
    <line x1="325" y1="216" x2="422" y2="352"/>
    <line x1="325" y1="296" x2="256" y2="448"/>
    <line x1="256" y1="336" x2="90"  y2="352"/>
    <line x1="187" y1="296" x2="90"  y2="160"/>
    <line x1="187" y1="216" x2="256" y2="64"/>
  </g>
  <circle cx="256" cy="256" r="40" fill="#F472B6"/>
</svg>
```

---

## 5. Facet

### Description
A regular hexagon centered on the tile, partitioned into six equilateral triangles meeting at the center. Each triangle is filled with a different flat shade of indigo, arranged so the lighter shades cluster at the top-right and the darker shades at the bottom-left — producing a faceted-crystal appearance through *tonal arrangement only*, never gradients. Each face is a single flat color. A near-black outer field provides high contrast and lets the facet float.

### Color Palette
| Role | Hex |
|---|---|
| Field | `#0B0B0F` (Obsidian) |
| Facet 1 (top-right) | `#A5B4FC` (Lightest Indigo) |
| Facet 2 (right) | `#818CF8` |
| Facet 3 (bottom-right) | `#6366F1` |
| Facet 4 (bottom-left) | `#4F46E5` |
| Facet 5 (left) | `#4338CA` |
| Facet 6 (top-left) | `#3730A3` (Darkest Indigo) |

### Symbolism
A polished crystal is the natural metaphor for what a structured note system *produces*: many flat surfaces, each catching the same light differently, all cut from one continuous material. The hexagon is the densest space-filling shape — it is what notes look like when packed without waste. The single-hue ramp signals that the variety in the app comes from one underlying material (your markdown), not from many disconnected systems. There is no literal letter, no quill, no lock; the icon is unmistakably *something well-made*.

### Small Size Strategy
The hexagonal silhouette and the diagonal light-to-dark ramp are robust. At 32×32 all six facets remain distinguishable; at 16×16 the eye perceives "a faceted hex with a light side and a dark side" — which is enough to identify the app on a Dock or in a window switcher. Because the contrast is internal (light facet vs dark facet), the icon doesn't depend on the surrounding background to read.

### SVG
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="112" fill="#0B0B0F"/>
  <polygon points="256,256 256,76  412,166" fill="#A5B4FC"/>
  <polygon points="256,256 412,166 412,346" fill="#818CF8"/>
  <polygon points="256,256 412,346 256,436" fill="#6366F1"/>
  <polygon points="256,256 256,436 100,346" fill="#4F46E5"/>
  <polygon points="256,256 100,346 100,166" fill="#4338CA"/>
  <polygon points="256,256 100,166 256,76"  fill="#3730A3"/>
  <polygon points="256,76 412,166 412,346 256,436 100,346 100,166"
           fill="none" stroke="#0B0B0F" stroke-width="3"/>
</svg>
```

---

## Summary

| # | Name | School | Dominant Color | Best At |
|---|---|---|---|---|
| 1 | Asterism | Minimalist / cartographic | Amber on Navy | Conveys "knowledge graph" without using the word |
| 2 | Sigil VI | Heraldic / typographic | Vermillion on Bone | Stamp-like memorability, brand asset versatility |
| 3 | Cartograph | Technical illustration | Mint on Forest | Quietly signals depth, precision, professionalism |
| 4 | Aperture | Industrial / mechanical | Teal + Magenta on Tungsten | Communicates focus and tooling |
| 5 | Facet | Geometric / abstract | Indigo ramp on Obsidian | Premium, modern, no narrative baggage |
