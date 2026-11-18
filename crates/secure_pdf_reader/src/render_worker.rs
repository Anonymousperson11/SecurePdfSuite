//! Single background render worker: page bitmaps and overlay frames, newest request wins.
//!
//! The worker thread owns nothing but the session and the UI thread's `DispatcherQueue`;
//! every `WinUI` object stays on the UI thread and is reached through
//! [`crate::reader_window::with_reader`] when a result is posted back.

use std::sync::{Arc, Condvar, Mutex};

use winui3::app::enqueue_ui;
use winui3::bindings::Microsoft::UI::Dispatching::DispatcherQueue;

use crate::logic::{OverlaySpec, ViewLayout, compute_view_layout, render_overlay};
use crate::reader_window::{apply_overlay_result, apply_page_result, with_reader};
use crate::session::ReaderSession;

/// Request to rasterize one page.
#[derive(Clone, Debug)]
pub struct PageJob {
    /// View generation the request belongs to; stale results are dropped on the UI thread.
    pub generation: u64,
    /// Zero-based page index.
    pub page_index: i32,
    /// Zoom factor (`1.0` = 100 %).
    pub zoom: f64,
    /// Display scale of the window (`1.0` at 96 DPI).
    pub dpi_scale: f64,
    /// Overlay to draw for this page so page and watermark reach the screen together; its
    /// `dpi_scale` is replaced by the bitmap's real pixels-per-DIP once the layout is known.
    pub overlay: OverlaySpec,
}

/// Request to draw one overlay frame matching the displayed page bitmap.
#[derive(Clone, Debug)]
pub struct OverlayJob {
    /// View generation the request belongs to.
    pub generation: u64,
    /// Overlay width in pixels (same as the page bitmap).
    pub px_width: i32,
    /// Overlay height in pixels.
    pub px_height: i32,
    /// What to draw.
    pub spec: OverlaySpec,
}

/// A page bitmap together with the overlay frame that belongs on top of it.
pub struct RenderedPage {
    /// Bitmap and display size.
    pub layout: ViewLayout,
    /// Premultiplied `BGRA8` page pixels.
    pub page_pixels: Vec<u8>,
    /// Premultiplied `BGRA8` overlay pixels (`None` = nothing to draw), or why the overlay
    /// could not be drawn.
    pub overlay_pixels: Result<Option<Vec<u8>>, String>,
}

/// Rendered page (or the reason it could not be produced).
pub struct PageResult {
    /// The request this answers.
    pub job: PageJob,
    /// The rendered page, or a user-facing error text.
    pub outcome: Result<RenderedPage, String>,
}

/// Rendered overlay frame (or the reason it could not be produced).
pub struct OverlayResult {
    /// View generation the frame belongs to.
    pub generation: u64,
    /// Overlay width in pixels.
    pub px_width: i32,
    /// Overlay height in pixels.
    pub px_height: i32,
    /// Premultiplied `BGRA8` pixels (`None` = nothing to draw), or a user-facing error text.
    pub pixels: Result<Option<Vec<u8>>, String>,
}

#[derive(Default)]
struct Queue {
    page: Option<PageJob>,
    overlay: Option<OverlayJob>,
    shutdown: bool,
}

/// Handle to the render worker thread.
pub struct RenderWorker {
    shared: Arc<(Mutex<Queue>, Condvar)>,
}

impl RenderWorker {
    /// Starts the worker thread for `session`, posting results to `dq`.
    #[must_use]
    pub fn spawn(session: Arc<ReaderSession>, dq: DispatcherQueue) -> Self {
        let shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        std::thread::spawn(move || run(&worker_shared, &session, &dq));
        Self { shared }
    }

    /// Queues a page render, replacing any page request that has not started yet.
    pub fn submit_page(&self, job: PageJob) {
        self.update(|q| q.page = Some(job));
    }

    /// Queues an overlay frame, replacing any overlay request that has not started yet.
    pub fn submit_overlay(&self, job: OverlayJob) {
        self.update(|q| q.overlay = Some(job));
    }

    /// Drops pending work and lets the thread exit (it releases its session reference).
    pub fn shutdown(&self) {
        self.update(|q| {
            q.page = None;
            q.overlay = None;
            q.shutdown = true;
        });
    }

    fn update(&self, change: impl FnOnce(&mut Queue)) {
        let (lock, signal) = &*self.shared;
        if let Ok(mut queue) = lock.lock() {
            change(&mut queue);
        }
        signal.notify_one();
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn next_jobs(shared: &(Mutex<Queue>, Condvar)) -> Option<(Option<PageJob>, Option<OverlayJob>)> {
    let (lock, signal) = shared;
    let mut queue = lock.lock().ok()?;
    loop {
        if queue.shutdown {
            return None;
        }
        if queue.page.is_some() || queue.overlay.is_some() {
            return Some((queue.page.take(), queue.overlay.take()));
        }
        queue = signal.wait(queue).ok()?;
    }
}

fn run(shared: &(Mutex<Queue>, Condvar), session: &ReaderSession, dq: &DispatcherQueue) {
    while let Some((page, overlay)) = next_jobs(shared) {
        if let Some(job) = page {
            let Some(outcome) = render_page(session, &job) else {
                // Plaintext already released: nothing may be rendered any more.
                return;
            };
            let result = PageResult { job, outcome };
            let _ = enqueue_ui(dq, move || {
                with_reader(|ui| apply_page_result(ui, result));
            });
        }
        if let Some(job) = overlay {
            let pixels =
                render_overlay(job.px_width, job.px_height, &job.spec).map_err(|e| e.to_string());
            let result = OverlayResult {
                generation: job.generation,
                px_width: job.px_width,
                px_height: job.px_height,
                pixels,
            };
            let _ = enqueue_ui(dq, move || {
                with_reader(|ui| apply_overlay_result(ui, result));
            });
        }
    }
}

/// Sizes and rasterizes the page and its overlay; `None` once the plaintext has been released.
fn render_page(session: &ReaderSession, job: &PageJob) -> Option<Result<RenderedPage, String>> {
    let page = session.with_pdf(|pdf| {
        let (width_pts, height_pts) = pdf
            .page_size_points(job.page_index)
            .map_err(|e| e.to_string())?;
        let layout = compute_view_layout(width_pts, height_pts, job.zoom, job.dpi_scale);
        let pixels = pdf
            .render_page_bgra(job.page_index, layout.px_width, layout.px_height)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>((layout, pixels))
    })?;
    Some(page.map(|(layout, page_pixels)| {
        let spec = OverlaySpec {
            dpi_scale: layout.effective_scale(),
            ..job.overlay.clone()
        };
        let overlay_pixels =
            render_overlay(layout.px_width, layout.px_height, &spec).map_err(|e| e.to_string());
        RenderedPage {
            layout,
            page_pixels,
            overlay_pixels,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay_spec(hidden: bool) -> OverlaySpec {
        OverlaySpec {
            hidden_watermark: hidden,
            visible_watermark: false,
            moire_level: secure_pdf_core::MoireLevel::Off,
            moire_phase: 0,
            watermark_text: "IP:192.168.1.20 | 26-11-16 18:30:45 | YVN8-2N88-7RN5".to_owned(),
            ipv4: "192.168.1.20".to_owned(),
            unix_seconds: 1_794_825_045,
            // Deliberately wrong: the worker must substitute the bitmap's real scale.
            dpi_scale: 7.0,
        }
    }

    fn page_job(dpi_scale: f64, hidden: bool) -> PageJob {
        PageJob {
            generation: 1,
            page_index: 0,
            zoom: 1.0,
            dpi_scale,
            overlay: overlay_spec(hidden),
        }
    }

    #[test]
    fn page_render_uses_the_dpi_aware_layout_and_stops_after_release() {
        let session = crate::session::test_support::session(0, "", 1);
        let job = page_job(1.5, false);
        let rendered = render_page(&session, &job).unwrap().unwrap();
        // A4 at 100 % on a 150 % display: one bitmap pixel per screen pixel.
        assert_eq!(
            (rendered.layout.px_width, rendered.layout.px_height),
            (1190, 1684)
        );
        assert_eq!(rendered.page_pixels.len(), 1190 * 1684 * 4);
        // A blank page renders opaque white from corner to corner.
        assert_eq!(&rendered.page_pixels[..4], &[255, 255, 255, 255]);
        assert_eq!(
            &rendered.page_pixels[rendered.page_pixels.len() - 4..],
            &[255, 255, 255, 255]
        );
        // Nothing enabled: no overlay bitmap at all.
        assert_eq!(rendered.overlay_pixels, Ok(None));

        let missing_page = PageJob {
            page_index: 5,
            ..page_job(1.5, false)
        };
        assert!(render_page(&session, &missing_page).unwrap().is_err());

        session.release_plaintext();
        assert!(render_page(&session, &job).is_none());
    }

    #[test]
    fn rendered_page_comes_with_a_decodable_overlay_at_the_real_display_scale() {
        use secure_pdf_core::ForensicsImage;
        use secure_pdf_core::watermark::{
            composite_overlay_over_page, decode_microdots, microdot_unit_px,
        };

        let session = crate::session::test_support::session(0, "", 1);
        let rendered = render_page(&session, &page_job(2.0, true))
            .unwrap()
            .unwrap();
        let (w, h) = (rendered.layout.px_width, rendered.layout.px_height);
        assert_eq!((w, h), (1587, 2245));
        let overlay = rendered
            .overlay_pixels
            .unwrap()
            .expect("hidden layer enabled");

        // What the screen shows: the PDFium page (BGRA) with the overlay on top.
        let page_rgba: Vec<u8> = rendered
            .page_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|px| [px[2], px[1], px[0], 255])
            .collect();
        let shown = ForensicsImage {
            width: u32::try_from(w).unwrap(),
            height: u32::try_from(h).unwrap(),
            rgba: composite_overlay_over_page(&page_rgba, &overlay),
        };
        let found = decode_microdots(&shown).expect("microdots must decode");
        assert_eq!(found.payload.ipv4, std::net::Ipv4Addr::new(192, 168, 1, 20));
        assert_eq!(found.payload.unix_seconds, 1_794_825_045);
        // Dot size follows the 200 % display, not the placeholder scale in the request.
        assert_eq!(found.unit_px, microdot_unit_px(2.0));
    }

    #[test]
    fn newest_request_replaces_pending_one_and_shutdown_clears_the_queue() {
        let shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let worker = RenderWorker {
            shared: Arc::clone(&shared),
        };
        for generation in 1..=3 {
            worker.submit_page(PageJob {
                generation,
                page_index: i32::try_from(generation).unwrap(),
                ..page_job(1.0, false)
            });
        }
        let (page, overlay) = next_jobs(&shared).unwrap();
        assert_eq!(page.map(|j| (j.generation, j.page_index)), Some((3, 3)));
        assert!(overlay.is_none());

        worker.submit_page(PageJob {
            generation: 4,
            ..page_job(1.0, false)
        });
        worker.shutdown();
        assert!(next_jobs(&shared).is_none());
        let (page_pending, overlay_pending, stopped) = {
            let queue = shared.0.lock().unwrap();
            (
                queue.page.is_some(),
                queue.overlay.is_some(),
                queue.shutdown,
            )
        };
        assert!(!page_pending && !overlay_pending && stopped);
    }
}
