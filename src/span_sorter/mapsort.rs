use ::array_init::array_init;
use image::{Rgb, RgbImage};

/// Spans up to this length are sorted with a simple stable sort instead of a counting sort,
/// because setting up the counting table would cost more than the sort itself.
const SMALL_SPAN: usize = 32;
/// Values are clamped to this maximum (hue is in [0, 360])
const MAX_VALUE: usize = 360;

/// Sorts a slice of Pixels by either Hue, Brightness or Saturation.
///
/// This is a stable counting sort: pixels with the same value keep their relative order,
/// which gives exactly the same result as the previous "map array" implementation,
/// but without allocating 360 vectors for every single span.
pub fn mapsort_mut(pixels: &mut [Rgb<u8>], get_pixel_value: for<'a> fn(&'a Rgb<u8>) -> u16) {
    let n = pixels.len();
    if n < 2 {
        return;
    }
    if n <= SMALL_SPAN {
        let mut wrapped: Vec<(u16, Rgb<u8>)> = pixels.iter().map(|p| (get_pixel_value(p), *p)).collect();
        // sort_by_key is stable
        wrapped.sort_by_key(|w| w.0);
        for (dst, src) in pixels.iter_mut().zip(wrapped) {
            *dst = src.1;
        }
        return;
    }

    let values: Vec<u16> = pixels
        .iter()
        .map(|p| get_pixel_value(p).min(MAX_VALUE as u16))
        .collect();

    // starts[v] = index where the first pixel with value v has to go
    let mut starts = [0usize; MAX_VALUE + 2];
    for &v in &values {
        starts[v as usize + 1] += 1;
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }

    let mut sorted: Vec<Rgb<u8>> = vec![Rgb([0, 0, 0]); n];
    for (p, &v) in pixels.iter().zip(values.iter()) {
        let pos = &mut starts[v as usize];
        sorted[*pos] = *p;
        *pos += 1;
    }
    pixels.copy_from_slice(&sorted);
}

/// Sorts all pixels of an image by either Hue, Brightness or Saturation.
/// It uses an array with 360 elements to map the values.
/// Probably panics if a compare function returns a value above 360
pub fn mapsort(img: &RgbImage, width: u32, height: u32, get_pixel_value: for<'a> fn(&'a Rgb<u8>) -> u16) -> RgbImage {
    let pixels = img.pixels();
    let mut sorted: RgbImage = RgbImage::new(width, height);
    let mut map_array: [Vec<&Rgb<u8>>; 360] = array_init(|_| Vec::new());

    for p in pixels {
        map_array[get_pixel_value(&p) as usize].push(&p);
    }

    let mut ind = 0;
    for h in map_array {
        for p in h {
            sorted.put_pixel(ind % width, ind / width, *p);
            ind += 1;
        }
    }

    return sorted;
}
