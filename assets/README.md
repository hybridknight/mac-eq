# MacEQ artwork

Original SVG artwork drawn for MacEQ, using the supplied equalizer-console image as visual reference. All shapes, highlights, gradients and controls are vectors; no embedded raster images, fonts, scripts, or remote resources.

- `AppIcon.svg`: 1024 × 1024 app mark with a full-bleed opaque charcoal background, blue and pink dials and four faders. macOS supplies the outer icon mask; artwork has no inset case or transparent outer margin, which would produce a smaller icon inside a second tile on macOS 26. The in-app header applies its own rounded clip.
- `Artwork.svg`: 1200 × 1000 presentation artwork on a pale pink/lavender/blue background.
- `MenuBarTemplate.svg`: 24 × 24 monochrome outline mark. macOS uses it as an 18-point template image and supplies the appropriate system tint in light and dark appearances.

`../scripts/icons.sh` renders native PNG sizes using AppKit and packages `bundle/generated/MacEQ.icns`. Generated images are checked in so a normal Rust build does not need the rendering step. The bundle script refreshes exports when SVG sources change and installs the icon plus SVG resources before signing.

The app icon is used for the application bundle, running app/Dock image, window icon configuration and header. The menu-bar SVG is embedded by the Rust build script so the custom mark also works outside the bundle; native raster exports provide a compatibility fallback. Artwork.svg remains available as a standalone asset and is included in the application resources.
