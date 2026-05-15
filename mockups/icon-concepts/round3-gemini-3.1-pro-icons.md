# Notesmith Icon Concepts — Round 3 (Gemini 3.1 Pro)

These concepts push into unexplored aesthetic territories for Notesmith, strictly adhering to a flat, modern, and minimal design language fitting for technical professionals. 

---

## 1. Concept: The Filing Labyrinth
**Description:** A clean, orthogonal, geometric labyrinth composed of thick, precisely spaced lines. The path begins at the outer edge and navigates through sharp 90-degree turns to reach a solid, luminous square at the center. 
**Color Palette:** Obsidian Black (`#121212`), Electric Cyan (`#00E5FF`), Slate Gray (`#607D8B`).
**Symbolism:** Represents the routing engine and the structured organization of complex, interconnected thoughts. Finding the exact note in a massive vault.
**Small Size Strategy (16×16):** The labyrinth's inner tracks are removed, leaving only the outer boundary and the glowing central cyan square to maintain legibility.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#121212" />
  <g fill="none" stroke="#607D8B" stroke-width="48" stroke-linecap="square">
    <!-- Outer maze structure -->
    <path d="M104,104 L408,104 L408,408 L200,408 L200,304 L304,304" />
    <path d="M104,408 L104,204 L304,204 L304,304" />
  </g>
  <!-- Central Destination -->
  <rect x="232" y="232" width="48" height="48" fill="#00E5FF" />
</svg>
```

---

## 2. Concept: Tectonic Strata
**Description:** A series of highly structured, stacked horizontal blocks in varying lengths, aligned to a strict invisible grid. The blocks resemble redacted text, code blocks, or geological layers.
**Color Palette:** Charcoal (`#1F2326`), Goldenrod (`#FCAF17`), Rust (`#E35205`), Slate (`#4B5C6B`).
**Symbolism:** Represents the foundational layers of knowledge built up over time (daily notes, incremental thoughts) forming a rock-solid vault of information. Let the markdown format stack.
**Small Size Strategy (16×16):** Reduces to three thick horizontal lines of alternating colors, resembling a highly stylized document or hamburger menu.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="112" fill="#1F2326" />
  <g stroke="none">
    <rect x="120" y="140" width="272" height="48" fill="#4B5C6B" />
    <rect x="120" y="212" width="160" height="48" fill="#FCAF17" />
    <rect x="300" y="212" width="92" height="48" fill="#4B5C6B" />
    <rect x="120" y="284" width="200" height="48" fill="#E35205" />
    <rect x="120" y="356" width="100" height="48" fill="#4B5C6B" />
  </g>
</svg>
```

---

## 3. Concept: Origami Data Plane
**Description:** A sequence of sharp, flat, polygonal vector shapes that interlock to form the silhouette of an abstract paper plane or dart, drawn in a sterile, dark-themed Swiss-design aesthetic.
**Color Palette:** Deep Navy (`#0A192F`), Bold Teal (`#64FFDA`), Pale Azure (`#E6F1FF`).
**Symbolism:** Origami represents transforming flat, raw materials (plain text/markdown) into highly functional, structured tools. The plane represents capturing and routing tasks.
**Small Size Strategy (16×16):** Becomes a simple, sharp triangle pointing top-right (the classic arrow of direction/action).

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#0A192F" />
  <polygon points="120,400 400,112 320,400 240,320" fill="#64FFDA" />
  <polygon points="120,400 240,320 120,240" fill="#E6F1FF" />
  <polygon points="400,112 240,320 220,200" fill="#E6F1FF" />
</svg>
```

---

## 4. Concept: The Penrose Vault
**Description:** A strictly flat, orthogonal representation of an impossible geometric shape (like a Penrose triangle). Built utilizing three distinct interlocking planes facing separate isometric directions. 
**Color Palette:** Dark Plum (`#311B92`), Electric Magenta (`#F50057`), Muted Coral (`#FF8A65`), Matte Black (`#000000`).
**Symbolism:** The paradoxical structure represents connecting disparate ideas and resolving complex, multi-dimensional tasks into a singular framework (the vault).
**Small Size Strategy (16×16):** Collapses into a basic solid hexagon or a stylized 'Y' intersecting cube-shape, preserving the isometric optical illusion.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#000000" />
  <g transform="translate(140, 110)">
    <!-- Flat isometric pieces of the triangle -->
    <polygon points="0,0 220,0 180,80 40,80 40,240 0,240" fill="#F50057" />
    <polygon points="220,0 220,240 180,240 180,80" fill="#FF8A65" />
    <polygon points="0,240 220,240 180,320 0,320 40,320 40,240" fill="#311B92" />
  </g>
</svg>
```

---

## 5. Concept: The Bracket Gateway
**Description:** A massive, monolithic abstraction of Markdown syntax (`[` and `]`). The brackets are thick, perfectly symmetrical, and interlocking, creating a negative space in the center that forms a subtle doorway or pillar.
**Color Palette:** Pitch Black (`#000000`), Neon Green (`#39FF14`), Pure White (`#FFFFFF`).
**Symbolism:** Direct homage to wikilinks `[[…]]` and code syntax, representing the entry point into the interconnected knowledge vault.
**Small Size Strategy (16×16):** Stays perfectly legible. The thin line weighting just becomes a solid 2px outline of opposing brackets.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#000000" />
  <!-- Left Bracket -->
  <path d="M224,112 L128,112 L128,400 L224,400" fill="none" stroke="#39FF14" stroke-width="56" stroke-linecap="square" stroke-linejoin="miter" />
  <!-- Right Bracket -->
  <path d="M288,112 L384,112 L384,400 L288,400" fill="none" stroke="#FFFFFF" stroke-width="56" stroke-linecap="square" stroke-linejoin="miter" />
  <!-- Central crossbar bridge, blending them -->
  <rect x="224" y="228" width="64" height="56" fill="#39FF14" />
</svg>
```