//! Real-font raster regression for button labels (not the mock test renderer).
//!
//! Run via `npm run test:typography` on Windows. The two columns are the
//! production label and an unmasked reference at the same font size. They must
//! contain the same glyph rows and ink; layout-only tests cannot detect this.

use gpui::{
    AppContext, Bounds, Context, IntoElement, Pixels, Render, Window, WindowBounds, WindowOptions,
    canvas, div, point, prelude::*, px, relative, size,
};
use gpui_ai::ButtonLabelExt as _;
use gpui_component::{ActiveTheme, Root, Sizable as _, Theme, button::Button, h_flex, v_flex};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type Measured = Rc<RefCell<Vec<Bounds<Pixels>>>>;

struct Probe {
    measured: Measured,
    small: bool,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut surface = v_flex().size_full().p_4().gap_4().bg(cx.theme().background);
        for (row_ix, label) in ["Theme: Sunday Panel", "gyjpq ÅÉ ﬁ ﬂ ffi"]
            .into_iter()
            .enumerate()
        {
            let mut row = h_flex().gap_4();
            for reference in [false, true] {
                let button = Button::new(format!("{row_ix}-{reference}"))
                    .outline()
                    .w_56()
                    .when(self.small, |b| b.small())
                    .on_click(|_, _, _| {});
                let button = if reference {
                    button.accessibility_label(label).child(
                        div()
                            .min_w_0()
                            .whitespace_nowrap()
                            .line_height(relative(1.))
                            .child(label),
                    )
                } else {
                    button.text_label(label)
                };
                let measured = self.measured.clone();
                row = row.child(
                    div().relative().child(button).child(
                        canvas(
                            move |bounds, _, _| measured.borrow_mut().push(bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
                );
            }
            surface = surface.child(row);
        }
        surface
    }
}

fn main() {
    let failed = Arc::new(AtomicBool::new(false));
    let result = failed.clone();
    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(move |cx| {
        gallery::init(cx);
        let slug = std::env::args().nth(1).unwrap_or_else(|| "sunday-panel".into());
        let theme = gallery::GalleryTheme::from_slug(&slug).expect("known test theme");
        gallery::apply_gallery_theme(theme, None, cx);
        if let Some(font_size) = std::env::args().nth(2).filter(|s| s != "default") {
            Theme::global_mut(cx).font_size = px(font_size.parse().expect("numeric rem override"));
            Theme::sync_base(cx);
        }
        let small = std::env::args().nth(3).is_some_and(|s| s == "small");
        let viewport = size(cx.theme().font_size * 32., cx.theme().font_size * 12.);
        let measured: Measured = Default::default();
        let shared = measured.clone();
        let handle = cx.open_window(WindowOptions {
            show: false,
            focus: false,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), viewport))),
            ..Default::default()
        }, move |window, cx| {
            let probe = cx.new(|_| Probe { measured: shared, small });
            cx.new(|cx| Root::new(probe, window, cx))
        }).expect("raster test window");
        cx.update_window(handle.into(), |_, window, _| window.resize(viewport)).expect("resize test window");
        // Process-lifetime test task: ends by quitting this hidden application.
        cx.spawn(async move |cx| {
            // Allow the native resize event before reading the hidden surface.
            cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            cx.update(|cx| {
                cx.update_window(handle.into(), |_, window, cx| {
                    window.bounds_changed(cx);
                    window.draw(cx).clear(cx);
                    measured.borrow_mut().clear();
                    window.refresh();
                    window.draw(cx).clear(cx);
                    let scale = window.scale_factor();
                    let image = window.render_to_image().expect("real native raster readback");
                    assert!(image.width() > 100, "native surface must be resized before sampling");
                    let bounds = measured.borrow();
                    assert_eq!(bounds.len(), 4, "two labels and their unmasked references");
                    // Windows readback excludes non-client chrome. Calibrate its
                    // inset from the first painted border rather than hardcoding it.
                    let first = bounds[0];
                    let left = (f32::from(first.left()) * scale).round() as u32;
                    let width = (f32::from(first.size.width) * scale).round() as u32;
                    let background = image.get_pixel(0, 0).0;
                    let frame_top = (0..image.height()).find(|y| {
                        (left + width / 4..left + width * 3 / 4).all(|x| {
                            let pixel = image.get_pixel(x, *y).0;
                            (0..3).map(|i| pixel[i].abs_diff(background[i]) as u32).sum::<u32>() > 5
                        })
                    }).expect("painted button border");
                    let offset = (f32::from(first.top()) * scale).round() as u32 - frame_top;
                    let ink = |bounds: Bounds<Pixels>| {
                        let x0 = (f32::from(bounds.left()) * scale).round() as u32;
                        let y0 = (f32::from(bounds.top()) * scale).round() as u32 - offset;
                        let x1 = (f32::from(bounds.right()) * scale).round() as u32;
                        let y1 = (f32::from(bounds.bottom()) * scale).round() as u32 - offset;
                        let inset = (2. * scale).ceil() as u32;
                        let bg = image.get_pixel((x0 + x1) / 2, y0 + inset).0;
                        let side = (y1 - y0) / 2; // exclude rounded border corners
                        let (mut rows, mut pixels) = (0, 0);
                        for y in y0 + inset..y1 - inset {
                            let count = (x0 + side..x1 - side).filter(|x| {
                                let pixel = image.get_pixel(*x, y).0;
                                (0..3).map(|i| pixel[i].abs_diff(bg[i]) as u32).sum::<u32>() > 120
                            }).count();
                            rows += usize::from(count > 0);
                            pixels += count;
                        }
                        (rows, pixels)
                    };
                    for pair in bounds.as_chunks::<2>().0 {
                        assert_eq!(pair[0].size, pair[1].size, "label must not change button geometry");
                        let actual = ink(pair[0]);
                        let reference = ink(pair[1]);
                        assert!(reference.0 > 0 && reference.1 > 0, "reference must contain visible glyphs");
                        if actual != reference {
                            eprintln!("{slug} rem={} small={small}: ink {actual:?} != unmasked {reference:?}", f32::from(window.rem_size()));
                            result.store(true, Ordering::Relaxed);
                        }
                    }
                }).expect("read test window");
                cx.quit();
            });
        }).detach();
    });
    if failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
}
