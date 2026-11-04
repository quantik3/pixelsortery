use image::Rgb;

#[derive(Debug)]
struct PixelWrapper {
    ind: usize,
    val: u16,
}

fn glitch_swap(
    pixels: &mut [Rgb<u8>],
    wrapper_vec: &mut Vec<PixelWrapper>,
    x: usize,
    y: usize,
) {
    // "Just" swap the Wrapper Objects in the Array and then change the color of the objects pixels

    let i = wrapper_vec[x].ind;
    let j = wrapper_vec[y].ind;

    wrapper_vec.swap(x, y);
    // Swaps the colors at i and j (the pixels are values now, not pointers)
    pixels.swap(i, j);

    // The glitch comes from the fact that
    // We switch wrap_x and wrap_y and then swap their colors
    // But they still point to their initial index
}

pub fn glitchsort_mut(pixels: &mut [Rgb<u8>], value_function: for<'a> fn(&'a Rgb<u8>) -> u16) {
    if pixels.len() < 2 {
        return;
    }
    let span_len = pixels.len() - 1;
    let mut wrappers = Vec::with_capacity(pixels.len());

    for i in 0..pixels.len() {
        let val = value_function(&pixels[i]);
        wrappers.push(PixelWrapper { ind: i, val });
    }

    let mut gap = span_len;
    let mut swapped = false;
    while (gap > 1 || swapped) {
        if (gap > 1) {
            gap = (gap as f64 / 1.247330950103979) as usize;
        }
        swapped = false;
        for i in 0..span_len {
            if (gap + i >= span_len) {
                break;
            }
            if (wrappers[i + gap].val as i8) > wrappers[i].val as i8 {
                glitch_swap(pixels, &mut wrappers, i + gap, i);
                swapped = true;
            }
        }
    }
}
