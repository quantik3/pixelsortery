use image::Rgb;

use crate::color_helpers;
mod glitchsort;
mod mapsort;
mod partial;
mod random_color;
mod shellsort;

#[derive(Debug, Clone, Copy)]
pub struct SpanSorter {
    pub criteria: SortingCriteria,
    pub algorithm: SortingAlgorithm,
    /// Number of passes for the partial sort (more passes = more sorted)
    pub passes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortingCriteria {
    Hue,
    Brightness,
    Saturation,
    Red,
    Green,
    Blue,
    /// Red-ish (warm) vs blue-ish (cold)
    Warmth,
    /// Perceived lightness (CIE L*)
    Lightness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortingAlgorithm {
    Mapsort,
    Shellsort,
    Glitchsort,
    /// Only a few passes of sorting: pixels just "drift" towards their place
    Partial,
    DebugColor,
}

impl SpanSorter {
    // Create new SpanSorter with sorting criteria and algorithm
    pub fn new(criteria: SortingCriteria) -> SpanSorter {
        SpanSorter {
            criteria,
            algorithm: SortingAlgorithm::Mapsort,
            passes: 8,
        }
    }
    pub fn info_string(&self) -> String {
        format!("Sorting pixel intervals by [{:?}] using [{:?}]", self.criteria, self.algorithm)
    }

    // Set criteria of SpanSorter
    pub fn set_criteria(&mut self, criteria: SortingCriteria) {
        self.criteria = criteria;
    }

    /// Returns the function that extracts the value we sort by.
    ///
    /// For example when sorting by Hue, this returns the function that calculates the hue of a pixel.
    pub fn get_value_function(criteria: SortingCriteria) -> for<'a> fn(&'a Rgb<u8>) -> u16 {
        match criteria {
            SortingCriteria::Brightness => color_helpers::get_brightness,
            SortingCriteria::Saturation => color_helpers::get_saturation,
            SortingCriteria::Red => color_helpers::get_red,
            SortingCriteria::Green => color_helpers::get_green,
            SortingCriteria::Blue => color_helpers::get_blue,
            SortingCriteria::Warmth => color_helpers::get_warmth,
            SortingCriteria::Lightness => color_helpers::get_lightness,
            SortingCriteria::Hue => color_helpers::get_hue,
        }
    }

    // Choose fitting algorithm for criteria
    // Idk why. This is dumb.
    pub fn determine_algorithm(&mut self) {
        self.algorithm = SortingAlgorithm::Mapsort;
    }

    /// Sort a slice of pixels using set criteria and algorithm
    pub fn sort(&self, pixels: &mut [Rgb<u8>]) {
        if self.algorithm == SortingAlgorithm::Partial {
            if pixels.len() >= 2 {
                partial::partial_sort_mut(pixels, SpanSorter::get_value_function(self.criteria), self.passes);
            }
            return;
        }
        // Select function per algorithm
        let sorting_function = match self.algorithm {
            SortingAlgorithm::DebugColor => random_color::set_random_color,
            SortingAlgorithm::Mapsort => mapsort::mapsort_mut,
            SortingAlgorithm::Shellsort => shellsort::shellsort_mut,
            SortingAlgorithm::Glitchsort => glitchsort::glitchsort_mut,
            // Handled above
            SortingAlgorithm::Partial => mapsort::mapsort_mut,
        };
        // Use a special, flawed brightness function for glitchsorting
        let criteria_function = match (self.algorithm, self.criteria) {
            (SortingAlgorithm::Glitchsort, SortingCriteria::Brightness) => color_helpers::get_brightness_flawed,
            _ => SpanSorter::get_value_function(self.criteria),
        };
        match self.algorithm {
            // Apply debug color even on every span (the call is below)
            SortingAlgorithm::DebugColor => {},
            // Skip sorting a span if it contains less than 2 pixels
            _ => if pixels.len() < 2 {return;},
        }
        // call sorting function
        sorting_function(pixels, criteria_function);
    }
}
