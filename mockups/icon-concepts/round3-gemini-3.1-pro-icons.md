# Notesmith App Icon Concepts - Round 3 (Flat & Abstract)

These 5 icon concepts explore structural and abstract metaphors (origami, Swiss grids, isometric layouts, impossible shapes, and sedimentary layers) while strictly maintaining a dark, professional Flat UI visual profile. No gradients are used.

## 1. The Origami Crane (Knowledge Forged from Paper)

**Description:** A sharply faceted origami form that resembles a folded crane or an abstract geometric bird diving downwards. It represents the transformation of flat, raw text ("paper") into structured, crafted knowledge ("smithing"). 
**Color Palette:** Deep Slate background (`#1E1E1E`), Cyan folds (`#0EA5E9`), Azure core (`#0284C7`), Navy shadow (`#0369A1`), Sky accent (`#38BDF8`).
**Symbolism:** Taking raw, flat material and meticulously shaping it into something purposeful and beautiful.
**Small Size Strategy:** The stark contrast between the bright blue central converging points and the dark background ensures it reads as a sharp geometric diamond/bird at 16px.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <!-- Background -->
  <rect width="512" height="512" rx="116" fill="#1E1E1E"/>
  <!-- Left Wing -->
  <polygon points="256,120 120,256 256,380" fill="#0EA5E9"/>
  <!-- Right Wing -->
  <polygon points="256,120 392,256 256,380" fill="#0284C7"/>
  <!-- Inner Crease / Shadow Left -->
  <polygon points="256,120 256,380 180,280" fill="#0369A1"/>
  <!-- Bottom Flap / Accent -->
  <polygon points="120,256 256,380 200,420" fill="#38BDF8"/>
</svg>
```

---

## 2. The Swiss Grid Labyrinth

**Description:** Inspired by asymmetrical Swiss grid design systems and labyrinths. It features a stark arrangement of flat rectangular forms that suggest columns of text, dynamic panes, and navigation paths. A single amber block breaks the monochrome, representing the user's active focus.
**Color Palette:** Charcoal background (`#121212`), Amber highlight (`#F59E0B`), Slate blocks (`#52525B`, `#3F3F46`, `#27272A`), Fog connector (`#71717A`).
**Symbolism:** Navigating large, structured spaces of information to find the golden nugget of insight.
**Small Size Strategy:** The single bright orange square against heavily contrasted asymmetrical blocks remains extremely recognizable even when scaled down to a favicon.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <!-- Background -->
  <rect width="512" height="512" rx="116" fill="#121212"/>
  <!-- Top Left Block -->
  <rect x="120" y="120" width="120" height="40" fill="#52525B"/>
  <!-- Central Vertical Shaft -->
  <rect x="200" y="120" width="40" height="272" fill="#71717A"/>
  <!-- Bottom Left Block -->
  <rect x="120" y="200" width="120" height="192" fill="#3F3F46"/>
  <!-- Top Right Focus Accent -->
  <rect x="280" y="120" width="112" height="120" fill="#F59E0B"/>
  <!-- Bottom Right Block -->
  <rect x="280" y="280" width="112" height="112" fill="#27272A"/>
</svg>
```

---

## 3. The Isometric Ascend

**Description:** An isometric view of extruded blocks resembling architecture or a staircase. It represents a structured, foundational approach to note-taking where bases are built upon bases.
**Color Palette:** Midnight Blue background (`#0F172A`), Emerald top (`#10B981`), Sea green left (`#059669`), Deep green right (`#047857`), Mint floater (`#34D399`).
**Symbolism:** Layered thoughts building an enduring foundation.
**Small Size Strategy:** The prominent 'Y' intersection of the three tones of the central isometric cube provides a foolproof 3D read at minimal pixel heights.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <!-- Background -->
  <rect width="512" height="512" rx="116" fill="#0F172A"/>
  <!-- Base Cube: Right Face -->
  <polygon points="256,256 346,206 346,316 256,366" fill="#047857"/>
  <!-- Base Cube: Left Face -->
  <polygon points="166,206 256,256 256,366 166,316" fill="#059669"/>
  <!-- Base Cube: Top Face -->
  <polygon points="256,156 346,206 256,256 166,206" fill="#10B981"/>
  <!-- Floating Top Face Accent -->
  <polygon points="256,100 300,125 256,150 212,125" fill="#34D399"/>
</svg>
```

---

## 4. The Impossible Ribbon

**Description:** An optical illusion resembling a Penrose triangle constructed of folded flat bands. It visually links multiple sides into an enclosed loop, depicting the non-linear, infinitely connectable nature of a markdown vault's knowledge graph.
**Color Palette:** Carbon background (`#171717`), Magenta bright (`#DB2777`), Rose base (`#BE185D`), Burgundy shadow (`#9D174D`).
**Symbolism:** The infinite, interconnected paths of thought spanning a centralized repository.
**Small Size Strategy:** The triangular loop of the ribbon remains strong, with the three distinct flat shades making it easily parseable as a unified geometric emblem ring.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <!-- Background -->
  <rect width="512" height="512" rx="116" fill="#171717"/>
  <!-- Bottom Ribbon Band -->
  <polygon points="120,380 392,380 342,280 170,280" fill="#BE185D"/>
  <!-- Left Ribbon Band -->
  <polygon points="120,380 256,108 306,108 220,280" fill="#DB2777"/>
  <!-- Right Ribbon Band Overlapping -->
  <polygon points="392,380 256,108 206,108 292,280" fill="#9D174D"/>
</svg>
```

---

## 5. The Sedimentary Stratum

**Description:** Hard-angled, diagonal intersecting planes of warm colors built strictly from solid flat blocks. This creates a visual of sedimentary geological layers, suggesting the compounding, timeless storage of accumulated notes.
**Color Palette:** Deep Umber background (`#1C1917`), Yellow Gold top (`#FBBF24`), Copper mid (`#D97706`), Rust low (`#B45309`), Cocoa base (`#78350F`).
**Symbolism:** Note-taking is an accumulative, historical mapping of thoughts where the old supports the new.
**Small Size Strategy:** Stacking contrasting flat bands creates a highly recognizable diagonal "slash" texture that stays vibrant even on busy dock backgrounds or at 16x16 scales.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <!-- Background -->
  <rect width="512" height="512" rx="116" fill="#1C1917"/>
  <!-- Deep Base Layer -->
  <path d="M 90 400 L 422 330 L 422 450 L 90 450 Z" fill="#78350F"/>
  <!-- Lower Mid Layer -->
  <path d="M 70 320 L 442 240 L 422 330 L 90 400 Z" fill="#B45309"/>
  <!-- Upper Mid Layer -->
  <path d="M 110 220 L 410 150 L 442 240 L 70 320 Z" fill="#D97706"/>
  <!-- Top Highlight Solid Layer -->
  <path d="M 140 140 L 390 90 L 410 150 L 110 220 Z" fill="#FBBF24"/>
</svg>
```
