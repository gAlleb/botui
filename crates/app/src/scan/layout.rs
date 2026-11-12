//! Zoom a fitted spread as one canvas, clipping only at the reading pane.
use crate::cover::Crop;
use ratatui::layout::{Rect, Size};

#[derive(Clone, Copy, Debug)]
pub(super) struct Placement {
    pub screen: Rect,
    pub window: [f32; 4],
}

pub(super) fn pages(
    area: Rect,
    cell: Size,
    dimensions: &[(u32, u32)],
    gap: u16,
    crop: Crop,
    align_height: bool,
) -> Vec<Option<Placement>> {
    if area.is_empty() || dimensions.is_empty() {
        return vec![None; dimensions.len()];
    }
    let (sizes, gap) = sizes(area, cell, dimensions, gap, crop, align_height);
    let width = sizes.iter().map(|s| s.0).sum::<f64>() + f64::from(gap);
    let origin = |viewport: u16, content: f64, pan: u8| {
        let extra = f64::from(viewport) - content;
        if extra >= 0. {
            (extra / 2.).floor()
        } else {
            (extra * f64::from(pan) / 255.).round()
        }
    };
    let mut x = origin(area.width, width, crop.x);
    sizes
        .into_iter()
        .map(|(w, h)| {
            let y = origin(area.height, h, crop.y);
            let left = x.max(0.).min(f64::from(area.width));
            let right = (x + w).max(0.).min(f64::from(area.width));
            let top = y.max(0.).min(f64::from(area.height));
            let bottom = (y + h).max(0.).min(f64::from(area.height));
            let result = (right > left && bottom > top).then(|| Placement {
                screen: Rect::new(
                    area.x + left as u16,
                    area.y + top as u16,
                    (right - left) as u16,
                    (bottom - top) as u16,
                ),
                window: [
                    ((left - x) / w) as f32,
                    ((top - y) / h) as f32,
                    ((right - x) / w) as f32,
                    ((bottom - y) / h) as f32,
                ],
            });
            x += w + f64::from(gap);
            result
        })
        .collect()
}

fn sizes(
    area: Rect,
    cell: Size,
    dimensions: &[(u32, u32)],
    gap: u16,
    crop: Crop,
    align_height: bool,
) -> (Vec<(f64, f64)>, u16) {
    let gap = if dimensions.len() > 1 {
        gap.min(area.width.saturating_sub(2))
    } else {
        0
    };
    if !align_height {
        let slot = f64::from(area.width.saturating_sub(gap)) / dimensions.len() as f64;
        let sizes: Vec<_> = dimensions
            .iter()
            .map(|&(w, h)| {
                let w = f64::from(w.max(1)) / f64::from(cell.width.max(1));
                let h = f64::from(h.max(1)) / f64::from(cell.height.max(1));
                let fit = (slot / w).min(f64::from(area.height) / h);
                let zoom = fit * f64::from(crop.percent) / 100.;
                ((w * zoom).round().max(1.), (h * zoom).round().max(1.))
            })
            .collect();
        return (sizes, gap);
    }
    // Facing pages share one physical height. Independent slot fitting makes
    // slightly different scans jump vertically and breaks a joined spread.
    let ratios: Vec<_> = dimensions
        .iter()
        .map(|&(w, h)| {
            f64::from(w.max(1)) / f64::from(h.max(1)) * f64::from(cell.height.max(1))
                / f64::from(cell.width.max(1))
        })
        .collect();
    let height = (f64::from(area.width.saturating_sub(gap)) / ratios.iter().sum::<f64>())
        .min(f64::from(area.height))
        * f64::from(crop.percent)
        / 100.;
    let sizes = ratios
        .into_iter()
        .map(|ratio| ((height * ratio).round().max(1.), height.round().max(1.)))
        .collect();
    (sizes, gap)
}

/// Centre a jumped-to annotation on the first page of the new spread.
pub(super) fn reveal(
    area: Rect,
    cell: Size,
    dimensions: &[(u32, u32)],
    gap: u16,
    mut crop: Crop,
    bounds: [f32; 4],
    align_height: bool,
) -> Crop {
    if dimensions.is_empty() || area.is_empty() {
        return crop;
    }
    let (sizes, gap) = sizes(area, cell, dimensions, gap, crop, align_height);
    let width = sizes.iter().map(|s| s.0).sum::<f64>() + f64::from(gap);
    let pan = |viewport: u16, canvas: f64, center: f64, fallback: u8| {
        if canvas <= f64::from(viewport) {
            fallback
        } else {
            ((center - f64::from(viewport) / 2.) / (canvas - f64::from(viewport)) * 255.)
                .round()
                .clamp(0., 255.) as u8
        }
    };
    crop.x = pan(
        area.width,
        width,
        sizes[0].0 * f64::from((bounds[0] + bounds[2]) / 2.),
        128,
    );
    crop.y = pan(
        area.height,
        sizes[0].1,
        sizes[0].1 * f64::from((bounds[1] + bounds[3]) / 2.),
        0,
    );
    crop
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jump_reveals_annotation_inside_a_zoomed_spread() {
        let area = Rect::new(20, 3, 120, 40);
        let cell = Size::new(8, 16);
        let dimensions = [(600, 1000), (600, 1000)];
        for percent in [100, 150, 250, 400] {
            for x in [0.05, 0.5, 0.95] {
                let crop = reveal(
                    area,
                    cell,
                    &dimensions,
                    0,
                    Crop {
                        percent,
                        ..Crop::default()
                    },
                    [x, 0.85, x, 0.85],
                    false,
                );
                let page = pages(area, cell, &dimensions, 0, crop, false)[0].unwrap();
                assert!(page.window[0] <= x && page.window[2] >= x);
                assert!(page.window[1] <= 0.85 && page.window[3] >= 0.85);
            }
        }
    }
    #[test]
    fn unequal_scans_share_the_top_and_bottom_of_the_spread() {
        let area = Rect::new(0, 0, 160, 48);
        let cell = Size::new(8, 16);
        for zoom in [100, 125, 200] {
            for pan in [0, 128, 255] {
                let spread = pages(
                    area,
                    cell,
                    &[(720, 1080), (720, 1104)],
                    0,
                    Crop {
                        percent: zoom,
                        y: pan,
                        ..Crop::default()
                    },
                    true,
                );
                let a = spread[0].unwrap();
                let b = spread[1].unwrap();
                assert_eq!(a.screen.y, b.screen.y);
                assert_eq!(a.screen.bottom(), b.screen.bottom());
                assert_eq!(a.screen.right(), b.screen.x);
                assert_eq!(a.window[1], b.window[1]);
                assert_eq!(a.window[3], b.window[3]);
            }
        }
    }
    #[test]
    fn zoom_uses_outer_margins_before_cropping_and_spread_gap_can_be_zero() {
        let area = Rect::new(20, 3, 120, 40);
        let cell = Size::new(8, 16);
        let dims = [(600, 1000), (600, 1000)];
        for gap in [0, 2, 32] {
            let fit = pages(area, cell, &dims, gap, Crop::default(), false);
            let a = fit[0].unwrap();
            let b = fit[1].unwrap();
            assert_eq!(b.screen.x - a.screen.right(), gap);
            assert_eq!(a.window, [0., 0., 1., 1.]);
        }
        let fit = pages(area, cell, &dims[..1], 0, Crop::default(), false)[0].unwrap();
        let zoom = pages(
            area,
            cell,
            &dims[..1],
            0,
            Crop {
                percent: 150,
                ..Crop::default()
            },
            false,
        )[0]
        .unwrap();
        assert!(zoom.screen.width > fit.screen.width);
        assert_eq!(zoom.window[0], 0.);
        assert_eq!(zoom.window[2], 1.);
        assert!(zoom.window[3] < 1.);
        for pan in [0, 128, 255] {
            let spread = pages(
                area,
                cell,
                &dims,
                0,
                Crop {
                    percent: 400,
                    x: pan,
                    y: pan,
                },
                false,
            );
            for placement in spread.iter().flatten() {
                assert_eq!(placement.screen.intersection(area), placement.screen);
                assert!(placement.window.iter().all(|v| (0. ..=1.).contains(v)));
            }
        }
    }
}
