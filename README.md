# SolidGear

SolidGear is a SolidCraft plugin for generating involute gears and racks as 3D solids. Open a tool from the **SolidGear** group in the **Plugins** ribbon tab, enter the dimensions, and click **Run**.

## Tools

| Tool | What it creates |
| --- | --- |
| **Involute Gear** | An external gear or a mating pair. Each gear can use a round, D-flat, or keyed bore and its own profile shift. |
| **Internal Involute Gear** | A ring gear with inward-facing teeth and an adjustable radial rim thickness. |
| **Involute Rack** | A straight rack with an adjustable tooth count and base height. |

All three tools support straight, helical, and double-helical teeth, adjustable backlash, and placement in X, Y, and Z with rotation about Z. Set **Helix angle** to `0°` for straight teeth. Enable **Double helix** with a nonzero angle to reverse the tooth direction at mid-width.

**Root fillet radius** rounds the tooth roots in the sketch. **Tooth-tip edge fillet**, available in **Involute Gear**, rounds the outer tips along both faces in 3D. Both settings default to `0 mm`.

### Example gear pair

Open **Involute Gear**, set **Gear 1 · Number of teeth** to `24`, **Module** to `2 mm`, and leave both profile shifts at `0`. Enable **Gear 2 · Generate second gear** and set its tooth count to `48`. With the first gear driving, the pair has a **2:1** reduction and **72 mm** between axes.

## Install

1. Build the plugin below, or use an existing `solidgear.zip` package.
2. In SolidCraft, open **Preferences → Plugins → Install plugin…**.
3. Select `solidgear.zip` and confirm installation.

## Build on Windows

Install Rust through rustup and run these commands in PowerShell from the project root.

```powershell
rustup target add wasm32-unknown-unknown
cargo install wasm-tools --locked
.\build.ps1
```

The script compiles the Rust plugin, wraps it as a WebAssembly component, and writes `solidgear.zip` to the project root. It also accepts a copy of `wasm-tools.exe` at `target/tools/bin/wasm-tools.exe`.

GitHub Actions also builds the plugin on pushes and pull requests. Open the **Build SolidGear plugin** workflow under **Actions** to run it manually or download `solidgear.zip` from a successful run.

## Publish a release

Push a version tag after committing the changes you want to release.

```powershell
git tag v0.2.0
git push origin v0.2.0
```

Once the tag build succeeds, the workflow attaches `solidgear.zip` under **Releases → Assets**. It creates the release if needed.

| Path | Purpose |
| --- | --- |
| [src/lib.rs](src/lib.rs) | Gear geometry and SolidCraft command calls |
| [solidgear/plugin.json](solidgear/plugin.json) | Tool definitions, input forms, and ribbon placement |
| [solidgear/icons](solidgear/icons) | SVG tool icons |
| [wit/plugin.wit](wit/plugin.wit) | SolidCraft WebAssembly plugin interface |

## Geometry notes

Some profiles are approximated with segments to keep generation practical. Large double-helical ring gears and 3D tooth-tip fillets can exceed the SolidCraft plugin execution limit.
