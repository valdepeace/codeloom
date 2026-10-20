# Third-party notices

## Martty wordmark, whale and preview fixture

The terminal wordmark rows in `crates/batten/src/widgets/logo.rs` are derived
from `src/logo.rs` in Martty. The `martty-whale.png` terminal raster in the
showcase is derived from `assets/tui-whale.svg`. The small attachment thumbnail fixture under
`crates/batten/examples/showcase/assets/` is cropped from Martty's
`assets/screenshots/image-preview.png`. Both are from the Martty repository,
licensed under MIT.

Copyright (c) 2026 OpenMA contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Pawan task queue

`crates/batten/src/widgets/queue.rs` and the `Pawan · task queue` showcase demo
are derived from `crates/pawan-cli/src/tui/queue_panel.rs` in Pawan. Pawan is
licensed under MIT.

Copyright (c) 2026 dirmacs

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## dotmax visual showcase dependency

The Batten showcase uses `dotmax` 0.1.11 as a development-only dependency to
show its progress-style catalog and Braille drawing, density, and color APIs in
the `Progress catalog` and `Braille graphics` demos. Optional image, SVG,
video, raytracer, and chess features remain disabled. No dotmax source code or
assets are copied into Batten. The crate is licensed under either MIT or
Apache-2.0; Cargo includes the selected crate's license files in the local
registry/cache.

Repository: <https://github.com/newjordan/dotmax>

## Crush logo reference

`crates/batten/examples/showcase/demos/crush_logo.rs` contains independent
Ratatui reference renderings of the wide title art, sidebar `Render` variant,
and low-height `SmallRender` fallback from `internal/ui/logo/logo.go` and
`internal/ui/model/sidebar.go` in the local Crush checkout. It does not copy
Crush implementation code or image assets. Crush is licensed under FSL-1.1-MIT,
which restricts competing use and does not grant general trademark rights.
These demos are internal UX references and must not be treated as clearance to
ship Crush code, assets, or branding in Batten.

Repository: <https://github.com/charmbracelet/crush>

## Mindfork logo and help tab strip

The geometry and fixed brand colors of `mindfork_glyph`, the pixel font and
layout of `mindfork_wordmark` / `mindfork_lockup`, and the active-tab and
overflow layout of `render_mindfork_tabs` are derived from `src/widgets/logo.rs`
and `src/widgets/help_dialog.rs` in Mindfork. The Mindfork repository is
licensed under MIT.

Copyright (c) 2026 Vladimir Shylov

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
