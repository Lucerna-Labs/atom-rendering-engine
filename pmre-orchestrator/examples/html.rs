//! HTML/CSS demo: an HTML document with inline CSS, reduced to the same math primitives
//! and rendered with no browser and no Vello.
//!
//! Run: cargo run -p pmre-orchestrator --example html

use pmre_kit::Rgba;
use pmre_orchestrator::render_html;

fn main() {
    let doc = r#"
<div style="display:flex; flex-direction:column; width:900px; height:560px; background:#15171d; gap:0">
  <div style="display:flex; flex-direction:row; height:54px; background:#1e2027; align-items:center; padding:18px; gap:24px">
    <span style="color:#eef2f8; font-size:18px">Browser</span>
    <span style="color:#9aa2b4; font-size:14px">Home</span>
    <span style="color:#9aa2b4; font-size:14px">Docs</span>
    <span style="color:#9aa2b4; font-size:14px">About</span>
  </div>
  <div style="display:flex; flex-direction:row; flex:1; gap:16px; padding:18px">
    <div style="display:flex; flex-direction:column; flex:1; background:#23262f; border-radius:12px; border:1px solid #363a46; padding:16px; gap:10px">
      <div style="width:44px; height:44px; border-radius:10px; background:#5c9ef6"></div>
      <h3 style="color:#eef2f8">Primitives</h3>
      <span style="color:#9aa2b4; font-size:13px">html and css reduced to math</span>
    </div>
    <div style="display:flex; flex-direction:column; flex:1; background:#23262f; border-radius:12px; border:1px solid #363a46; padding:16px; gap:10px">
      <div style="width:44px; height:44px; border-radius:10px; background:#34d399"></div>
      <h3 style="color:#eef2f8">Layout</h3>
      <span style="color:#9aa2b4; font-size:13px">flex box model solver</span>
    </div>
    <div style="display:flex; flex-direction:column; flex:1; background:#23262f; border-radius:12px; border:1px solid #363a46; padding:16px; gap:10px">
      <div style="width:44px; height:44px; border-radius:10px; background:#fbbf60"></div>
      <h3 style="color:#eef2f8">Raster</h3>
      <span style="color:#9aa2b4; font-size:13px">sdf coverage and glyphs</span>
    </div>
  </div>
  <div style="display:flex; flex-direction:column; flex:1; padding:18px">
    <div style="display:flex; flex-direction:column; flex:1; background:#20232b; border-radius:12px; border:1px solid #343845; padding:16px; gap:8px">
      <h3 style="color:#eef2f8">Status</h3>
      <span style="color:#9aa2b4; font-size:13px">rendered with no browser and no vello</span>
    </div>
  </div>
</div>
"#;

    let clear = Rgba::rgb8(21, 23, 29);
    let (w, h) = (900u32, 560u32);
    let fb = render_html(doc, w, h, clear);
    let bmp = fb.to_bmp(clear);
    let path = r"html.bmp";
    std::fs::write(path, bmp).expect("write html.bmp");
    println!("wrote {path} ({w}x{h})");
}
