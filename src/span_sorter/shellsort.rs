use image::Rgb;

pub fn shellsort_mut(pixels: &mut [Rgb<u8>], value_function: for<'a> fn(&'a Rgb<u8>) -> u16){
    // Stolen from some Stackoverflow Thread
    // (It's actually a comb sort. The result is kept exactly as before.)

    let span_len = pixels.len();
    // Pair each pixel with its calculated value
    let mut fake_pixels: Vec<(u16, Rgb<u8>)> = pixels.iter().map(|px| (value_function(px), *px)).collect();

    let mut gap = span_len;
    let mut swapped = false;
    while ( (gap > 1) || swapped ) {
        if (gap > 1){ gap = (gap as f64/1.247330950103979) as usize; }
        swapped = false;
        for i in 0..span_len {
            if (gap + i >= span_len){break;}
            if ( fake_pixels[i+gap].0 < fake_pixels[i].0 ){
                fake_pixels.swap(i+gap, i);
                swapped = true;
            }
        }
    }

    for (dst, src) in pixels.iter_mut().zip(fake_pixels) {
        *dst = src.1;
    }
}
