# Notesmith — Icon Concepts (Opus 4.7)

Five distinct flat icon concepts for Notesmith. Each is designed to read clearly from 16×16 to 1024×1024 and works on both light and dark surfaces.

---

## 1. The Forge

**Concept Name:** The Forge

**Description:** A stylized anvil sits at the base of a rounded-square tile. Floating just above the anvil's face is a single sheet of paper with a folded top-right corner and three short text lines. A sharp four-point spark sits between the anvil and the page, suggesting the moment a note is being "struck" into shape. The composition is centered, vertically balanced, and uses three flat tones plus one ember accent.

**Color Palette:**
- Background: `#1B2330` (deep slate)
- Anvil / steel: `#C8CDD6`
- Page: `#F5F2EA` (warm paper)
- Page rules: `#9AA3B2`
- Ember accent: `#FF6B35`

**Symbolism:** The literal "smith" metaphor — the anvil is the vault, the page is the note, the spark is the act of capture. Craftsmanship and intentionality.

**Small Size Strategy:** At 32×32 the spark and rules drop out; only the anvil silhouette and page rectangle remain, which together still read as "note on anvil." At 16×16 the anvil reduces to a single dark trapezoid with a white square above — still recognizable as a tool + page.

**SVG:**
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="96" fill="#1B2330"/>
  <!-- Anvil -->
  <path d="M64 312 L168 312 L168 296 L344 296 L344 312 L448 312 L448 332 L360 332 L344 348 L344 372 L376 388 L376 408 L136 408 L136 388 L168 372 L168 348 L152 332 L64 332 Z" fill="#C8CDD6"/>
  <!-- Spark -->
  <path d="M256 232 L264 256 L288 264 L264 272 L256 296 L248 272 L224 264 L248 256 Z" fill="#FF6B35"/>
  <!-- Page -->
  <path d="M152 88 L320 88 L368 136 L368 248 L152 248 Z" fill="#F5F2EA"/>
  <path d="M320 88 L320 136 L368 136 Z" fill="#D9D5C9"/>
  <rect x="184" y="152" width="128" height="10" rx="3" fill="#9AA3B2"/>
  <rect x="184" y="180" width="152" height="10" rx="3" fill="#9AA3B2"/>
  <rect x="184" y="208" width="96" height="10" rx="3" fill="#9AA3B2"/>
</svg>
```

---

## 2. Forged N

**Concept Name:** Forged N

**Description:** A bold geometric monogram "N" rendered as two thick vertical I-beams connected by a single diagonal stroke. The verticals are cool steel; the diagonal is a hot ember orange — the bar that was just struck. The "N" sits on a rounded-square dark tile with generous padding so the silhouette dominates. Corners of the verticals are subtly chamfered (still flat fills, just polygonal cuts) to evoke milled steel.

**Color Palette:**
- Background: `#0F1620` (near-black slate)
- Steel verticals: `#E6EAF0`
- Ember diagonal: `#FF6B35`
- Inner highlight band: `#FFB07A` (flat, not gradient)

**Symbolism:** A wordmark-as-icon. Treats the brand letter itself as a forged object — the diagonal is the freshly hammered piece. Communicates "tool, brand, precision" instantly without any literal note imagery.

**Small Size Strategy:** The N's silhouette is the entire icon. At 32×32 the chamfers and inner highlight collapse but the two verticals + diagonal still scan as an N. At 16×16 it reads as a strong monogram tile, the orange diagonal giving it brand color recognition even when shape detail blurs.

**SVG:**
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="96" fill="#0F1620"/>
  <!-- Left vertical (with chamfered top-left and bottom-right) -->
  <path d="M120 120 L184 120 L184 392 L168 408 L120 408 L120 136 Z" fill="#E6EAF0"/>
  <!-- Right vertical (with chamfered top-left and bottom-right) -->
  <path d="M328 120 L392 120 L392 392 L344 408 L328 408 L328 136 Z" fill="#E6EAF0"/>
  <!-- Diagonal (ember) -->
  <path d="M184 120 L264 120 L392 392 L392 408 L312 408 L184 136 Z" fill="#FF6B35"/>
  <!-- Inner highlight band on diagonal -->
  <path d="M212 152 L240 152 L356 388 L328 388 Z" fill="#FFB07A"/>
</svg>
```

---

## 3. Vault Stack

**Concept Name:** Vault Stack

**Description:** Three horizontally stacked note "cards" of equal width, each slightly offset to the right as they ascend, evoking an opened drawer of files. A vertical accent bar runs down the left side connecting all three, suggesting a spine or index. The topmost card carries a single small accent dot in its upper-left corner — the "active" note. Set on a clean rounded-square tile.

**Color Palette:**
- Background: `#1E2A38` (deep navy)
- Card 1 (back): `#3A5775`
- Card 2 (middle): `#4E7BA3`
- Card 3 (front): `#6FA0CC`
- Spine + accent dot: `#F4A261` (amber)

**Symbolism:** The vault as an organized stack of notes; the spine as the routing/index that ties everything together; the dot as the single note currently in focus. Speaks to power users who think in collections.

**Small Size Strategy:** At 32×32 the dot drops out and the three cards merge into a clear stair-step shape with the amber spine — still legible as "stacked files." At 16×16 it reduces to the staircase silhouette with one orange edge, which is distinctive and memorable.

**SVG:**
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="96" fill="#1E2A38"/>
  <!-- Spine -->
  <rect x="80" y="120" width="24" height="272" rx="8" fill="#F4A261"/>
  <!-- Card 1 (back) -->
  <rect x="112" y="312" width="288" height="80" rx="12" fill="#3A5775"/>
  <!-- Card 2 (middle) -->
  <rect x="128" y="216" width="288" height="80" rx="12" fill="#4E7BA3"/>
  <!-- Card 3 (front) -->
  <rect x="144" y="120" width="288" height="80" rx="12" fill="#6FA0CC"/>
  <!-- Accent dot on top card -->
  <circle cx="172" cy="148" r="10" fill="#F4A261"/>
</svg>
```

---

## 4. Linked Pages

**Concept Name:** Linked Pages

**Description:** Two square note pages overlap diagonally — one in the back-left, one in the front-right. Each has a folded corner. Where they overlap, a chain-link symbol is carved out as negative space, revealing the dark tile behind and binding the two notes together. The composition is asymmetric but balanced, oriented along a 45° axis. No outlines, only flat shapes.

**Color Palette:**
- Background: `#171F2B` (deep ink)
- Back page: `#4ECDC4` (teal)
- Front page: `#F5F2EA` (paper)
- Folded corners: `#3AAFA7` and `#D9D5C9` (one shade darker than each page)
- Link cutout: background color (`#171F2B`)

**Symbolism:** Backlinks and the connective tissue of a knowledge vault. Two notes, joined — the central metaphor of a linked-note system. The chain emerging from the overlap suggests links are *born* from notes touching one another.

**Small Size Strategy:** At 32×32 the chain-link cutout simplifies to a single dark slot through the overlap. At 16×16 it reads as two overlapping squares (one teal, one cream) with a dark notch between — still clearly two linked things.

**SVG:**
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="96" fill="#171F2B"/>
  <!-- Back page (teal) -->
  <path d="M88 88 L264 88 L312 136 L312 320 L88 320 Z" fill="#4ECDC4"/>
  <path d="M264 88 L264 136 L312 136 Z" fill="#3AAFA7"/>
  <!-- Front page (paper) -->
  <path d="M200 192 L376 192 L424 240 L424 424 L200 424 Z" fill="#F5F2EA"/>
  <path d="M376 192 L376 240 L424 240 Z" fill="#D9D5C9"/>
  <!-- Chain link (negative cutout) drawn as dark shapes over overlap -->
  <!-- Left ring -->
  <path d="M232 232 a40 40 0 1 1 0 80 a40 40 0 1 1 0 -80 z M232 252 a20 20 0 1 0 0 40 a20 20 0 1 0 0 -40 z" fill="#171F2B" fill-rule="evenodd"/>
  <!-- Right ring -->
  <path d="M296 232 a40 40 0 1 1 0 80 a40 40 0 1 1 0 -80 z M296 252 a20 20 0 1 0 0 40 a20 20 0 1 0 0 -40 z" fill="#171F2B" fill-rule="evenodd"/>
  <!-- Connecting bar between rings -->
  <rect x="240" y="262" width="48" height="20" fill="#171F2B"/>
</svg>
```

---

## 5. Quill Mark

**Concept Name:** Quill Mark

**Description:** A single, sharp downward chevron — the tip of a chisel or a quill nib — strikes the top edge of a rectangular note tile, leaving behind a small triangular notch and a single ember spark. The chevron is rendered as a solid flat shape in steel; the note tile beneath is paper-cream; the strike point glows orange. Centered composition on a dark rounded-square tile.

**Color Palette:**
- Background: `#1A2230` (slate)
- Chisel/nib: `#D7DCE4` (steel)
- Note tile: `#F5F2EA` (paper)
- Note ruled lines: `#A8B0BD`
- Strike spark: `#FF6B35`

**Symbolism:** The act of writing-as-forging. The chisel meeting the page is the moment a thought becomes a note. Combines the "smith" metaphor with the literal act of authoring — distinct from concept 1's anvil because the focus is on the **strike**, not the workshop.

**Small Size Strategy:** At 32×32 the rules disappear, leaving a clean silhouette: a steel triangle pointing into a cream rectangle with one orange dot. At 16×16 it collapses to a triangle-on-rectangle shape — still reads as "tool meeting paper."

**SVG:**
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="96" fill="#1A2230"/>
  <!-- Note tile -->
  <rect x="96" y="232" width="320" height="208" rx="20" fill="#F5F2EA"/>
  <!-- Ruled lines -->
  <rect x="136" y="296" width="240" height="10" rx="3" fill="#A8B0BD"/>
  <rect x="136" y="332" width="240" height="10" rx="3" fill="#A8B0BD"/>
  <rect x="136" y="368" width="168" height="10" rx="3" fill="#A8B0BD"/>
  <!-- Chisel / nib -->
  <path d="M208 72 L304 72 L304 200 L256 256 L208 200 Z" fill="#D7DCE4"/>
  <!-- Inner facet on chisel (flat darker plane) -->
  <path d="M256 88 L296 88 L296 196 L256 244 Z" fill="#B8BFC9"/>
  <!-- Notch in note where chisel strikes -->
  <path d="M232 232 L280 232 L256 256 Z" fill="#1A2230"/>
  <!-- Spark -->
  <circle cx="256" cy="268" r="10" fill="#FF6B35"/>
</svg>
```

---

**Brief notes for selecting between concepts:**
- Choose **The Forge** or **Quill Mark** if you want literal, narrative branding.
- Choose **Forged N** for strongest brand-mark recognition at small sizes (taskbar, favicon).
- Choose **Vault Stack** to communicate "organized notes" before "smith."
- Choose **Linked Pages** to lead with the linked-note value proposition.
