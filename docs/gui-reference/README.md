# GUI reference files

One JSON file per upstream window, generated from the WinForms `*.Designer.cs`
sources. Each file is the serialized reference model of that window: its title,
client size, and every control (name, type, parent, order, visible text, layout
properties, items, events) in the Designer's declaration order.

These files are the single source of truth for the 1:1 fidelity tests. They pin
the original control tree so a ported window can be checked automatically.

- **Pinned upstream commit:** `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`
  (`tools/fetch-upstream.sh`)
- **Regenerate:**
  `bash tools/fetch-upstream.sh && cargo test -p airscout --locked --test gui_reference -- --ignored generate_gui_reference`
- **Never edit by hand.** Regenerate from the upstream checkout only. Hand edits
  would defeat the fidelity check (see `docs/superpowers/specs/2026-11-26-gui-1to1-design.md`,
  section 4); justified differences belong in the per-window allowlist instead.
