//! One self-contained HTML file from the running demo ("Download as one
//! HTML file"): the page skeleton, the wasm-bindgen glue inlined, the wasm
//! and every loaded data file as base64 blocks, and the state to restore.
//! It opens from `file://` (no fetch, no module imports) and can itself be
//! downloaded again: the skeleton and the glue travel along as files.
//!
//! Everything here is plain Rust (tested natively); `mini_app` collects the
//! bytes in the browser and saves the result.

use std::io::Write;

/// Standard base64 (with padding).
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("in-memory gzip");
    enc.finish().expect("in-memory gzip")
}

/// Data files served raw (for HTTP brotli) get their payload gzipped inside
/// their own format again, as the readers accept both: `.globe` (8-byte
/// header, flag at 6), layers `.coords/.meta/.names` (20-byte header, flag
/// at 6), packed coastlines (5-byte header, gzip detected). Others pass.
pub fn gzip_inside(name: &str, bytes: &[u8]) -> Vec<u8> {
    let with_flag = |header: usize| {
        if bytes.len() > header && bytes[6] == 0 {
            let mut out = bytes[..header].to_vec();
            out[6] = 1;
            out.extend(gzip(&bytes[header..]));
            out
        } else {
            bytes.to_vec()
        }
    };
    if name.ends_with(".globe") && bytes.starts_with(b"GDBG") {
        with_flag(8)
    } else if (name.ends_with(".coords")
        || name.ends_with(".meta")
        || name.ends_with(".names")
        || name.ends_with(".fold")
        || name.ends_with(".foldhan")
        || name.ends_with(".foldhangul"))
        && bytes.starts_with(b"GDBL")
    {
        with_flag(20)
    } else if bytes.starts_with(b"GDBC")
        && bytes.len() > 5
        && !bytes[5..].starts_with(&[0x1f, 0x8b])
    {
        let mut out = bytes[..5].to_vec();
        out.extend(gzip(&bytes[5..]));
        out
    } else {
        bytes.to_vec()
    }
}

/// The wasm-bindgen glue as plain module code: no exports (an inline
/// module cannot be imported from anyway).
pub fn glue_script(js: &str) -> Result<String, String> {
    let mut out = String::with_capacity(js.len());
    for line in js.lines() {
        let t = line.trim_start();
        if t.starts_with("export {") {
            continue;
        }
        let line = [
            "export function ",
            "export class ",
            "export const ",
            "export let ",
            "export async function ",
        ]
        .iter()
        .find_map(|p| line.strip_prefix("export ").filter(|_| line.starts_with(p)))
        .unwrap_or(line);
        let t = line.trim_start();
        if t.starts_with("import ") || t.starts_with("export ") {
            return Err("the glue imports or exports more than the inliner handles".into());
        }
        out.push_str(line);
        out.push('\n');
    }
    // A literal "</script>" would end the inline script early.
    Ok(out.replace("</script>", "<\\/script>"))
}

/// Removes every `<tag ...>` … `</close>` block that starts with `open`.
fn remove_blocks(html: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(close) {
            Some(j) => {
                rest = &rest[i + j + close.len()..];
                // The line break that followed the block goes with it.
                rest = rest.strip_prefix('\n').unwrap_or(rest);
            }
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The page without its loader, preloads and any embedded files: what the
/// single file wraps again.
pub fn strip_page(html: &str) -> String {
    let html = remove_blocks(html, "<script type=\"module\">", "</script>");
    let html = remove_blocks(
        &html,
        "<script type=\"application/octet-stream\"",
        "</script>",
    );
    let html = remove_blocks(&html, "<link rel=\"modulepreload\"", ">");
    remove_blocks(&html, "<link rel=\"preload\"", ">")
}

/// Everything one single file carries.
pub struct Bundle<'a> {
    /// The stripped page (see [`strip_page`]).
    pub skeleton: &'a str,
    /// The wasm-bindgen glue, as served.
    pub glue: &'a str,
    pub wasm: &'a [u8],
    /// Data files by name (gzip inside where possible, see [`gzip_inside`]).
    pub files: &'a [(String, Vec<u8>)],
    /// A query string the app restores (`?layers=…&at=…`).
    pub state: &'a str,
    /// "webgpu" or "webgl2".
    pub build: &'a str,
}

const SIZE_PLACEHOLDER: &str = "__GEODB_FILE_BYTES_000000000";

/// The self-contained HTML.
pub fn build_html(b: &Bundle<'_>) -> Result<String, String> {
    let script = glue_script(b.glue)?;
    let mut blocks = String::new();
    let mut names = Vec::new();
    let mut add = |name: &str, bytes: &[u8], blocks: &mut String| {
        blocks.push_str(&format!(
            "<script type=\"application/octet-stream\" id=\"embed:{name}\">{}</script>\n",
            base64(bytes)
        ));
        names.push(name.to_string());
    };
    add("app.wasm", b.wasm, &mut blocks);
    add("app.js", b.glue.as_bytes(), &mut blocks);
    add("page.html", b.skeleton.as_bytes(), &mut blocks);
    for (name, bytes) in b.files {
        add(name, bytes, &mut blocks);
    }
    let map: Vec<String> = names
        .iter()
        .map(|n| format!("{n:?}: bytes({n:?})"))
        .collect();
    let loader = format!(
        "<script type=\"module\">\n{script}\n\
         const bytes = (id) => Uint8Array.from(atob(document.getElementById(\"embed:\" + id).textContent), (c) => c.charCodeAt(0));\n\
         window.__GEODB_FILE_BYTES = Number(\"{SIZE_PLACEHOLDER}\".replace(/\\D/g, \"\"));\n\
         window.__GEODB_STATE = {state:?};\n\
         window.__GEODB_BUILD = {build:?};\n\
         window.__GEODB_FILES = {{ js: \"app.js\", wasm: \"app.wasm\" }};\n\
         window.__GEODB_EMBEDDED = {{ {} }};\n\
         await __wbg_init({{ module_or_path: window.__GEODB_EMBEDDED[\"app.wasm\"] }});\n\
         </script>\n",
        map.join(", "),
        state = b.state,
        build = b.build,
    );
    let at = b
        .skeleton
        .rfind("</body>")
        .ok_or("the page has no </body>")?;
    let html = format!("{}{blocks}{loader}{}", &b.skeleton[..at], &b.skeleton[at..]);
    let size = html.len();
    Ok(html.replacen(
        SIZE_PLACEHOLDER,
        &format!("__GEODB_FILE_BYTES_{size:09}"),
        1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn raw_payloads_are_gzipped_inside_and_still_read() {
        let db = crate::data::db();
        let files = geodb_core::globe_layers::build_globe_files(db, 32, false, None).unwrap();
        let base = gzip_inside("cities.globe", &files.base);
        assert!(base.len() < files.base.len() / 2 && base[6] == 1);
        let mut globe = geodb_core::globe_db::CompactGlobeDb::from_bytes(&base).unwrap();
        let coords = gzip_inside("cities.coords", &files.coords);
        assert!(coords.len() < files.coords.len() && coords[6] == 1);
        globe.attach_layer(&coords).unwrap();
        // Already compressed stays as it is.
        assert_eq!(gzip_inside("cities.globe", &base), base);

        let ring = vec![vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0]]];
        let raw = crate::mini::pack_coast(&[&ring], false);
        let packed = gzip_inside("coast10m.bin", &raw);
        assert_eq!(
            crate::mini::unpack_coast(&packed).unwrap(),
            crate::mini::unpack_coast(&raw).unwrap()
        );
    }

    #[test]
    fn page_and_glue_are_inlined() {
        let page = "<html><head><link rel=\"modulepreload\" href=\"/a.js\"></head><body>x\
                    <script type=\"module\">import init from './a.js';</script>\
                    <script type=\"application/octet-stream\" id=\"embed:old\">AAAA</script></body></html>";
        let skeleton = strip_page(page);
        assert_eq!(skeleton, "<html><head></head><body>x</body></html>");
        let glue = "export function start() {}\nfunction __wbg_init() {}\nexport { initSync, __wbg_init as default };\n";
        assert_eq!(
            glue_script(glue).unwrap(),
            "function start() {}\nfunction __wbg_init() {}\n"
        );
        assert!(glue_script("import x from 'y';\n").is_err());

        let files = vec![("cities.globe".to_string(), vec![1u8, 2, 3])];
        let html = build_html(&Bundle {
            skeleton: &skeleton,
            glue,
            wasm: &[0, 97, 115, 109],
            files: &files,
            state: "?layers=meta&at=48.1,11.6,1.5",
            build: "webgpu",
        })
        .unwrap();
        assert!(html.ends_with("</body></html>"));
        assert!(html.contains("id=\"embed:cities.globe\">AQID</script>"));
        assert!(html.contains("window.__GEODB_STATE = \"?layers=meta&at=48.1,11.6,1.5\";"));
        let n: String = html
            .split("__GEODB_FILE_BYTES_")
            .nth(1)
            .unwrap()
            .chars()
            .take(9)
            .collect();
        assert_eq!(
            n.parse::<usize>().unwrap(),
            html.len(),
            "the page knows its size"
        );
        // The skeleton travels along, and stripping the result gives it back.
        assert_eq!(strip_page(&html), skeleton);
    }
}
