# Notesmith Icon Concepts — Gemini 3.1 Pro

## 1. The Anvil & Page
**Description:** A clean, geometric representation of an anvil where the base forms the shape of a folded page or document. The top is a solid, heavy horizontal bar (the anvil face), while the main body is styled like a crisp note with a single folded corner.
**Color Palette:** Iron Grey (`#2E3440`), Paper White (`#ECEFF4`), Accent Forge Orange (`#D08770`).
**Symbolism:** Merges the "smith" (crafting, structural, durable) with the "note" (markdown, document). It tells the story of forging raw ideas into solid documents.
**Small Size Strategy:** At 16x16, the folded corner simplifies out, leaving a strong, solid anvil silhouette that is immediately recognizable.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="100" fill="#2E3440" />
  <path d="M120 180h272v60H120z" fill="#D08770" />
  <path d="M176 240h160v160H176z" fill="#ECEFF4" />
  <path d="M336 400l40-40v-120h-40zM176 400l-40-40v-120h40z" fill="#D8DEE9" />
</svg>
```

## 2. The Abstract Node-Carabiner
**Description:** Focusing on the "linking" and capture nature of network-based notes, this icon uses a single continuous stroke that shapes a stylized "N" while doubling as a carabiner or chain link. The lines are thick and geometric.
**Color Palette:** Vault Slate (`#1E232B`), Primary Link Blue (`#4A90E2`), Cyan Accent (`#50E3C2`).
**Symbolism:** Represents securing (capture), linking (backlinks, connections), and durability. The "N" stands for Notesmith. It appeals to power users who understand nodal knowledge systems.
**Small Size Strategy:** The stroke thickness is intentionally heavy. At 32x32, it reduces perfectly to a clear, abstract "N" shape with distinct color blocking.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="100" fill="#1E232B" />
  <path d="M160 352V160l112 112V352z" fill="#4A90E2" />
  <path d="M240 160v192l112-112V160z" fill="#50E3C2" />
</svg>
```

## 3. The Marked Markdown
**Description:** A structural take on the traditional Markdown logo (the M with the downward arrow), merged into an isometric stack of layered vaults or plates. The M forms the ridges, while the arrow hits the center, representing capturing an idea perfectly into the vault.
**Color Palette:** Midnight Purple (`#1B1724`), Solid White (`#FFFFFF`), Amethyst Accent (`#9D72FF`).
**Symbolism:** The structure implies a multi-layered vault of knowledge, while the Markdown syntax M provides instant recognition for developer-oriented users.
**Small Size Strategy:** The lower layers disappear, prioritizing the stark M and the downward arrow in the center.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="100" fill="#1B1724" />
  <path d="M126 360V182h70l60 81 60-81h70v178h-48V250l-82 110-82-110v110h-48z" fill="#FFFFFF" />
  <path d="M236 410l20 20 20-20h-40z" fill="#9D72FF" />
</svg>
```

## 4. The Monospace Cursor
**Description:** Absolute minimalist design focusing on speed and terminal/command-line origins. It features a bold green monospace bracket `[` and a glowing solid block cursor `_` next to it, laid out to imply a document.
**Color Palette:** Console Black (`#0F1115`), Forge Emerald (`#10B981`), Silver (`#9CA3AF`).
**Symbolism:** Directly speaks to technical professionals. Writing markup, fast capture, keyboard-driven navigation. The cursor represents readiness to write.
**Small Size Strategy:** The bracket and block remain crisp and easily identifiable even at 16x16, resembling simple code syntax.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="100" fill="#0F1115" />
  <path d="M140 120v272h50v-40h-10V160h10v-40h-50z" fill="#9CA3AF" />
  <rect x="230" y="312" width="142" height="80" fill="#10B981" />
</svg>
```

## 5. The Triage Funnel
**Description:** Three distinct horizontal bands (representing different streams, tasks, or customer files) elegantly converging down into a central, glowing dot or single stack at the bottom. The shapes are clean, flat polygons.
**Color Palette:** Charcoal (`#1F2937`), Task Blue (`#3B82F6`), Stream Yellow (`#F59E0B`), Core White (`#F9FAFB`).
**Symbolism:** Embodies capture, routing, and organization. Transforming disorganized chaos (top bands) into structured clarity (single bottom stack). Perfect for the task triage and note routing aspects of Notesmith.
**Small Size Strategy:** The three colored bands stand out well, reducing simply to a three-line badge on small favicons.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="100" fill="#1F2937" />
  <path d="M120 140h272v40H120z" fill="#3B82F6" />
  <path d="M160 220h192v40H160z" fill="#F59E0B" />
  <path d="M220 300h72v40h-72z" fill="#F9FAFB" />
  <circle cx="256" cy="380" r="24" fill="#F9FAFB" />
</svg>
```
