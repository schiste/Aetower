# Aetower Design System

Status: scaffolded

Aetower is a power-user cockpit. UI code should be compact, data-dense, and
consistent. New styling belongs in the central design system first, then feature
views consume it.

## Source Of Truth

- `macos/Sources/AetowerUI/DesignSystem/DesignTokens.swift` owns spacing,
  radius, stroke, typography, motion, semantic colors, metric tones, and surface
  colors.
- `macos/Sources/AetowerUI/DesignSystem/AetowerComponents.swift` owns shared
  SwiftUI primitives such as surfaces, badges, metric readouts, empty states, and
  section shells.

## Default Rule

Feature views should not invent visual language. Prefer:

- `AetowerDesign.Spacing` instead of raw padding and spacing constants.
- `AetowerDesign.Typography` instead of direct `.font(...)` values.
- `AetowerDesign.Ink`, `AetowerDesign.Status`, and `AetowerDesign.Tone` instead
  of ad-hoc text/icon colors.
- `AetowerSurface`, `AetowerBadge`, and related primitives instead of raw
  `RoundedRectangle`, `Capsule`, custom overlays, or repeated surface styling.

## Gate Behavior

`scripts/quality-guard.py` now emits warning-only design-system drift findings
for changed SwiftUI lines in `AetowerApp` and `AetowerUI` outside
`AetowerUI/DesignSystem`.

Warnings do not block commits. They identify migration targets so the app can
move toward centralized styling without freezing ongoing product work.

Run a repo-wide advisory audit with:

```bash
python3 scripts/quality-guard.py --mode design-audit
```

The audit is intentionally warning-only. It prints a per-file summary with a few
examples so each tab can be migrated incrementally without making normal commits
noisy or impossible.

## Migration Policy

When a warning appears:

1. If the styling is repeated or likely reusable, add or extend a component in
   `AetowerUI/DesignSystem`.
2. If only a token is missing, add the token to `AetowerDesign`.
3. If a feature needs one-off styling, keep it local for now but leave the
   warning visible until the pattern proves stable or reusable.
