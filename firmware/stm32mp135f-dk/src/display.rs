//! The LCD through Linux DRM: two dumb buffers in RGB565 (the panel's format, the renderer's too), shown in
//! turn with a page flip on the vertical blank. The program needs the display to itself: Weston must be
//! stopped first (`systemctl stop weston-launch`).

use drm::buffer::{Buffer as _, DrmFourcc};
use drm::control::{connector, crtc, framebuffer, Device as ControlDevice, Event, PageFlipFlags};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsFd, BorrowedFd};

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl drm::Device for Card {}
impl ControlDevice for Card {}

/// One dumb buffer, mapped for the program's whole life.
struct Page {
    fb: framebuffer::Handle,
    px: &'static mut [u8],
    pitch: usize,
}

pub struct Display {
    card: Card,
    crtc: crtc::Handle,
    pages: [Page; 2],
    /// The page on the panel; the other one is drawn next.
    front: usize,
    /// A flip was asked for and has not happened yet.
    flipping: bool,
    pub width: usize,
    pub height: usize,
}

impl Display {
    /// Opens the card, takes the first connected connector at its preferred mode and shows a black page.
    pub fn open(path: &str) -> io::Result<Display> {
        let card = Card(OpenOptions::new().read(true).write(true).open(path)?);
        let res = card.resource_handles()?;
        let con = res
            .connectors()
            .iter()
            .filter_map(|&h| card.get_connector(h, true).ok())
            .find(|c| c.state() == connector::State::Connected)
            .ok_or_else(|| io::Error::other("no connected display"))?;
        let mode = *con
            .modes()
            .first()
            .ok_or_else(|| io::Error::other("the display has no mode"))?;
        let enc = con
            .current_encoder()
            .or_else(|| con.encoders().first().copied())
            .ok_or_else(|| io::Error::other("the display has no encoder"))?;
        let enc = card.get_encoder(enc)?;
        let crtc = enc
            .crtc()
            .or_else(|| res.filter_crtcs(enc.possible_crtcs()).first().copied())
            .ok_or_else(|| io::Error::other("no CRTC for the display"))?;
        let (w, h) = mode.size();
        let (width, height) = (usize::from(w), usize::from(h));
        let page = |card: &Card| -> io::Result<Page> {
            let buf = card.create_dumb_buffer((w.into(), h.into()), DrmFourcc::Rgb565, 16)?;
            let fb = card.add_framebuffer(&buf, 16, 16)?;
            let pitch = buf.pitch() as usize;
            // The buffers live as long as the program: leaked, so the mapping can be kept with them.
            let buf: &'static mut _ = Box::leak(Box::new(buf));
            let map = Box::leak(Box::new(card.map_dumb_buffer(buf)?));
            let px: &'static mut [u8] = map.as_mut();
            px.fill(0);
            Ok(Page { fb, px, pitch })
        };
        let pages = [page(&card)?, page(&card)?];
        card.set_crtc(crtc, Some(pages[0].fb), (0, 0), &[con.handle()], Some(mode))
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("set_crtc: {e} (is Weston still running?)"),
                )
            })?;
        Ok(Display {
            card,
            crtc,
            pages,
            front: 0,
            flipping: false,
            width,
            height,
        })
    }

    /// Copies the frame (RGB565, `width` x `height`) into the hidden page and flips to it on the next vertical
    /// blank. Waits for the flip before (the page it writes must be off the panel), not after.
    pub fn show(&mut self, frame: &[u16]) -> io::Result<()> {
        self.wait_flip()?;
        let back = 1 - self.front;
        let page = &mut self.pages[back];
        for (y, row) in frame.chunks_exact(self.width).take(self.height).enumerate() {
            let dst = &mut page.px[y * page.pitch..y * page.pitch + self.width * 2];
            for (d, s) in dst.chunks_exact_mut(2).zip(row) {
                d.copy_from_slice(&s.to_le_bytes());
            }
        }
        self.card
            .page_flip(self.crtc, page.fb, PageFlipFlags::EVENT, None)?;
        self.flipping = true;
        self.front = back;
        Ok(())
    }

    fn wait_flip(&mut self) -> io::Result<()> {
        while self.flipping {
            for event in self.card.receive_events()? {
                if let Event::PageFlip(_) = event {
                    self.flipping = false;
                }
            }
        }
        Ok(())
    }
}
