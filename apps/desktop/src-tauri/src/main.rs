// WebKitGTK's DMA-BUF renderer blanks or crashes the webview while the page is
// compositing continuously (for example while dragging a node) on some drivers,
// notably NVIDIA under Wayland, where it fails with a Wayland protocol error.
// Disable it by default; an explicit value from the caller still wins, and the
// caller can opt back in by exporting the variable.
fn should_disable_dmabuf(current: Option<&std::ffi::OsStr>) -> bool {
    current.is_none()
}

fn apply_webkit_render_fallbacks() {
    if should_disable_dmabuf(std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").as_deref()) {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    apply_webkit_render_fallbacks();
    rawweave_desktop::run();
}

#[cfg(test)]
mod tests {
    use super::should_disable_dmabuf;
    use std::ffi::OsStr;

    #[test]
    fn keeps_an_explicit_dmabuf_choice() {
        assert!(should_disable_dmabuf(None));
        assert!(!should_disable_dmabuf(Some(OsStr::new("0"))));
        assert!(!should_disable_dmabuf(Some(OsStr::new("1"))));
    }
}
