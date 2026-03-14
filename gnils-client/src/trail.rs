use bevy::image::Image;

/// Draw an anti-aliased line on a CPU-side RGBA image using Xiaolin Wu's algorithm.
/// The image must be Rgba8UnormSrgb format with known width/height.
pub fn draw_aa_line(image: &mut Image, x0: f64, y0: f64, x1: f64, y1: f64, color: (u8, u8, u8)) {
    let width = image.width() as i32;
    let height = image.height() as i32;
    let bpp = 4usize; // RGBA8

    let data = image.data.as_mut().unwrap();

    let mut plot = |x: i32, y: i32, brightness: f64| {
        if x < 0 || x >= width || y < 0 || y >= height {
            return;
        }
        let idx = ((y * width + x) as usize) * bpp;
        if idx + 3 >= data.len() {
            return;
        }
        let alpha = (brightness * 255.0).clamp(0.0, 255.0) as u8;
        let existing_a = data[idx + 3] as f64 / 255.0;
        let new_a = alpha as f64 / 255.0;
        let out_a = new_a + existing_a * (1.0 - new_a);
        if out_a > 0.0 {
            let blend = |old: u8, new: u8| -> u8 {
                ((new as f64 * new_a + old as f64 * existing_a * (1.0 - new_a)) / out_a) as u8
            };
            data[idx] = blend(data[idx], color.0);
            data[idx + 1] = blend(data[idx + 1], color.1);
            data[idx + 2] = blend(data[idx + 2], color.2);
            data[idx + 3] = (out_a * 255.0) as u8;
        }
    };

    let steep = (y1 - y0).abs() > (x1 - x0).abs();

    let (mut x0, mut y0, mut x1, mut y1) = if steep {
        (y0, x0, y1, x1)
    } else {
        (x0, y0, x1, y1)
    };

    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < 1e-10 { 1.0 } else { dy / dx };

    // First endpoint
    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xgap = 1.0 - (x0 + 0.5).fract();
    let xpxl1 = xend as i32;
    let ypxl1 = yend.floor() as i32;
    if steep {
        plot(ypxl1, xpxl1, (1.0 - yend.fract()) * xgap);
        plot(ypxl1 + 1, xpxl1, yend.fract() * xgap);
    } else {
        plot(xpxl1, ypxl1, (1.0 - yend.fract()) * xgap);
        plot(xpxl1, ypxl1 + 1, yend.fract() * xgap);
    }
    let mut intery = yend + gradient;

    // Second endpoint
    let xend2 = x1.round();
    let yend2 = y1 + gradient * (xend2 - x1);
    let xgap2 = (x1 + 0.5).fract();
    let xpxl2 = xend2 as i32;
    let ypxl2 = yend2.floor() as i32;
    if steep {
        plot(ypxl2, xpxl2, (1.0 - yend2.fract()) * xgap2);
        plot(ypxl2 + 1, xpxl2, yend2.fract() * xgap2);
    } else {
        plot(xpxl2, ypxl2, (1.0 - yend2.fract()) * xgap2);
        plot(xpxl2, ypxl2 + 1, yend2.fract() * xgap2);
    }

    // Main loop
    for x in (xpxl1 + 1)..xpxl2 {
        if steep {
            plot(intery.floor() as i32, x, 1.0 - intery.fract());
            plot(intery.floor() as i32 + 1, x, intery.fract());
        } else {
            plot(x, intery.floor() as i32, 1.0 - intery.fract());
            plot(x, intery.floor() as i32 + 1, intery.fract());
        }
        intery += gradient;
    }
}

/// Clear the trail canvas to fully transparent black.
pub fn clear_trail(image: &mut Image) {
    if let Some(data) = image.data.as_mut() {
        data.fill(0);
    }
}
