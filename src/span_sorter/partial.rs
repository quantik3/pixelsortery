use image::Rgb;

/// Sorts a slice of pixels only partially: `passes` rounds of odd-even transposition sort.
///
/// Every round compares neighbouring pixels and swaps them if they are in the wrong order,
/// so each pixel moves at most `passes` steps towards its place. With few passes this gives
/// a "dripping" look, with many passes the span becomes completely sorted.
pub fn partial_sort_mut(pixels: &mut [Rgb<u8>], value_function: for<'a> fn(&'a Rgb<u8>) -> u16, passes: u32) {
    let n = pixels.len();
    if n < 2 {
        return;
    }
    let mut wrapped: Vec<(u16, Rgb<u8>)> = pixels.iter().map(|p| (value_function(p), *p)).collect();
    // After n rounds the slice is sorted anyway
    let rounds = (passes as usize).min(n);
    for round in 0..rounds {
        let mut swapped = false;
        let mut i = round % 2;
        while i + 1 < n {
            if wrapped[i + 1].0 < wrapped[i].0 {
                wrapped.swap(i, i + 1);
                swapped = true;
            }
            i += 2;
        }
        // Already sorted: stop early (both kinds of rounds have to agree)
        if !swapped && round > 0 && is_sorted(&wrapped) {
            break;
        }
    }
    for (dst, src) in pixels.iter_mut().zip(wrapped) {
        *dst = src.1;
    }
}

fn is_sorted(v: &[(u16, Rgb<u8>)]) -> bool {
    v.windows(2).all(|w| w[0].0 <= w[1].0)
}
