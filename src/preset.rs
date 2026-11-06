//! Saving settings as text (presets) and random settings

use crate::{
    path_creator::{PathCreator, DEFAULT_CENTER},
    pixel_selector::{PixelSelectCriteria, PixelSelector},
    span_sorter::{SortingAlgorithm, SortingCriteria},
    ChannelMode, Pixelsorter,
};
use rand::Rng;
use std::fmt::Debug;

const ALL_CRITERIA: [SortingCriteria; 9] = [
    SortingCriteria::Hue,
    SortingCriteria::Brightness,
    SortingCriteria::Saturation,
    SortingCriteria::Red,
    SortingCriteria::Green,
    SortingCriteria::Blue,
    SortingCriteria::Warmth,
    SortingCriteria::Lightness,
    SortingCriteria::Guide,
];
const ALL_ALGORITHMS: [SortingAlgorithm; 7] = [
    SortingAlgorithm::Mapsort,
    SortingAlgorithm::Shellsort,
    SortingAlgorithm::Glitchsort,
    SortingAlgorithm::Partial,
    SortingAlgorithm::Stretch,
    SortingAlgorithm::Shift,
    SortingAlgorithm::DebugColor,
];
const ALL_CHANNELS: [ChannelMode; 4] = [ChannelMode::All, ChannelMode::Red, ChannelMode::Green, ChannelMode::Blue];
const ALL_SELECT_CRITERIA: [PixelSelectCriteria; 3] =
    [PixelSelectCriteria::Hue, PixelSelectCriteria::Brightness, PixelSelectCriteria::Saturation];

/// Finds the variant whose name (as printed with {:?}) is `name`, ignoring case
fn parse_name<T: Copy + Debug>(all: &[T], name: &str) -> Result<T, String> {
    all.iter()
        .copied()
        .find(|v| format!("{:?}", v).eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("Unknown value '{}'", name))
}

fn parse_num<T: std::str::FromStr>(s: Option<&str>, what: &str) -> Result<T, String> {
    let s = s.ok_or_else(|| format!("Missing value for {}", what))?;
    s.trim().parse::<T>().map_err(|_| format!("Invalid number '{}' for {}", s, what))
}

fn path_to_string(p: PathCreator) -> String {
    match p {
        PathCreator::AllHorizontally => "horizontal".into(),
        PathCreator::AllVertically => "vertical".into(),
        PathCreator::HorizontalLines => "right".into(),
        PathCreator::VerticalLines => "down".into(),
        PathCreator::Rays => "rays".into(),
        PathCreator::Circles => "circles".into(),
        PathCreator::Spiral => "spiral".into(),
        PathCreator::SquareSpiral => "spiral-square".into(),
        PathCreator::RectSpiral => "spiral-rect".into(),
        PathCreator::Diagonally(a) => format!("diagonal:{}", a),
        PathCreator::Hilbert => "hilbert".into(),
        PathCreator::X3(a, b, c) => format!("x3:{}:{}:{}", a, b, c),
        PathCreator::Waves(a, l, horizontal) => format!("waves:{}:{}:{}", a, l, horizontal as u8),
        PathCreator::FlowField(s, seed) => format!("flow:{}:{}", s, seed),
        PathCreator::Drawn => "drawn".into(),
        PathCreator::Star(p, i) => format!("star:{}:{}", p, i),
    }
}

fn path_from_string(s: &str) -> Result<PathCreator, String> {
    let mut parts = s.split(':');
    let name = parts.next().unwrap_or_default().trim().to_lowercase();
    let mut next = |what: &str| parts.next().map(|p| p.to_string()).ok_or_else(|| format!("Missing value for {}", what));
    Ok(match name.as_str() {
        "horizontal" => PathCreator::AllHorizontally,
        "vertical" => PathCreator::AllVertically,
        "right" => PathCreator::HorizontalLines,
        "down" => PathCreator::VerticalLines,
        "rays" => PathCreator::Rays,
        "circles" => PathCreator::Circles,
        "spiral" => PathCreator::Spiral,
        "spiral-square" => PathCreator::SquareSpiral,
        "spiral-rect" => PathCreator::RectSpiral,
        "hilbert" => PathCreator::Hilbert,
        "drawn" => PathCreator::Drawn,
        "diagonal" => PathCreator::Diagonally(parse_num(Some(&next("angle")?), "angle")?),
        "x3" => PathCreator::X3(
            parse_num(Some(&next("x3")?), "x3")?,
            parse_num(Some(&next("x3")?), "x3")?,
            parse_num(Some(&next("x3")?), "x3")?,
        ),
        "waves" => PathCreator::Waves(
            parse_num(Some(&next("amplitude")?), "amplitude")?,
            parse_num(Some(&next("wavelength")?), "wavelength")?,
            parse_num::<u8>(Some(&next("direction")?), "direction")? != 0,
        ),
        "flow" => PathCreator::FlowField(parse_num(Some(&next("size")?), "size")?, parse_num(Some(&next("seed")?), "seed")?),
        "star" => PathCreator::Star(parse_num(Some(&next("points")?), "points")?, parse_num(Some(&next("inner")?), "inner")?),
        other => return Err(format!("Unknown path '{}'", other)),
    })
}

fn selector_to_string(s: PixelSelector) -> String {
    match s {
        PixelSelector::Full => "full".into(),
        PixelSelector::Fixed { len } => format!("fixed:{}", len),
        PixelSelector::Random { max } => format!("random:{}", max),
        PixelSelector::Threshold { min, max, criteria } => format!("thres:{:?}:{}:{}", criteria, min, max),
        PixelSelector::Edges { threshold } => format!("edges:{}", threshold),
    }
}

fn selector_from_string(s: &str) -> Result<PixelSelector, String> {
    let parts: Vec<&str> = s.split(':').collect();
    Ok(match parts[0].trim().to_lowercase().as_str() {
        "full" => PixelSelector::Full,
        "fixed" => PixelSelector::Fixed { len: parse_num(parts.get(1).copied(), "fixed")? },
        "random" => PixelSelector::Random { max: parse_num(parts.get(1).copied(), "random")? },
        "edges" => PixelSelector::Edges { threshold: parse_num(parts.get(1).copied(), "edges")? },
        "thres" => PixelSelector::Threshold {
            criteria: parse_name(&ALL_SELECT_CRITERIA, parts.get(1).copied().unwrap_or_default())?,
            min: parse_num(parts.get(2).copied(), "threshold min")?,
            max: parse_num(parts.get(3).copied(), "threshold max")?,
        },
        other => return Err(format!("Unknown selector '{}'", other)),
    })
}

impl Pixelsorter {
    /// All settings as one line of text, e.g. to save them as a preset
    /// (`path=down;selector=thres:Brightness:60:200;criteria=Brightness;...`).
    pub fn to_preset_string(&self) -> String {
        format!(
            "path={};selector={};criteria={:?};algorithm={:?};reverse={};center={}:{};alternate={};passes={};amount={};channels={:?};posterize={}",
            path_to_string(self.path_creator),
            selector_to_string(self.selector),
            self.sorter.criteria,
            self.sorter.algorithm,
            self.reverse as u8,
            self.path_center.0,
            self.path_center.1,
            self.alternate as u8,
            self.sorter.passes,
            self.sorter.amount,
            self.channels,
            self.posterize,
        )
    }

    /// Reads settings written by `to_preset_string`. Settings that are missing keep their default,
    /// unknown keys are ignored (so presets of newer versions still load).
    pub fn from_preset_string(text: &str) -> Result<Pixelsorter, String> {
        let mut ps = Pixelsorter::new();
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let (key, value) = part.split_once('=').ok_or_else(|| format!("Expected key=value, got '{}'", part))?;
            let value = value.trim();
            match key.trim() {
                "path" => ps.path_creator = path_from_string(value)?,
                "selector" => ps.selector = selector_from_string(value)?,
                "criteria" => ps.sorter.criteria = parse_name(&ALL_CRITERIA, value)?,
                "algorithm" => ps.sorter.algorithm = parse_name(&ALL_ALGORITHMS, value)?,
                "reverse" => ps.reverse = parse_num::<u8>(Some(value), "reverse")? != 0,
                "center" => {
                    let (x, y) = value.split_once(':').ok_or("Expected center=x:y")?;
                    ps.path_center = (parse_num(Some(x), "center x")?, parse_num(Some(y), "center y")?);
                }
                "alternate" => ps.alternate = parse_num::<u8>(Some(value), "alternate")? != 0,
                "passes" => ps.sorter.passes = parse_num(Some(value), "passes")?,
                "amount" => ps.sorter.amount = parse_num(Some(value), "amount")?,
                "channels" => ps.channels = parse_name(&ALL_CHANNELS, value)?,
                "posterize" => ps.posterize = parse_num(Some(value), "posterize")?,
                _ => {}
            }
        }
        Ok(ps)
    }

    /// Random settings, for finding unexpected effects. The paths "Drawn" and the criteria "Guide"
    /// are left out (they need extra input), debug colors too.
    pub fn randomize(&mut self) {
        let mut rng = rand::thread_rng();
        let paths = [
            PathCreator::AllHorizontally,
            PathCreator::AllVertically,
            PathCreator::HorizontalLines,
            PathCreator::VerticalLines,
            PathCreator::Rays,
            PathCreator::Circles,
            PathCreator::Spiral,
            PathCreator::SquareSpiral,
            PathCreator::RectSpiral,
            PathCreator::Diagonally(rng.gen_range(0.0..360.0f32).round()),
            PathCreator::Hilbert,
            PathCreator::Waves(rng.gen_range(5.0..80.0f32).round(), rng.gen_range(40.0..600.0f32).round(), rng.gen_bool(0.5)),
            PathCreator::FlowField(rng.gen_range(30.0..800.0f32).round(), rng.gen_range(0..100_000u32)),
            PathCreator::Star(rng.gen_range(3..12u32), (rng.gen_range(0.2..1.0f32) * 100.0).round() / 100.0),
        ];
        self.path_creator = paths[rng.gen_range(0..paths.len())];
        self.reverse = rng.gen_bool(0.5);
        self.path_center = if rng.gen_bool(0.5) {
            DEFAULT_CENTER
        } else {
            ((rng.gen_range(0.2..0.8f32) * 100.0).round() / 100.0, (rng.gen_range(0.2..0.8f32) * 100.0).round() / 100.0)
        };

        let low = rng.gen_range(0..180u64);
        self.selector = match rng.gen_range(0..4u32) {
            0 => PixelSelector::Fixed { len: rng.gen_range(10..800u64) },
            1 => PixelSelector::Random { max: rng.gen_range(10..800u32) },
            2 => PixelSelector::Edges { threshold: rng.gen_range(20..250u64) },
            _ => PixelSelector::Threshold {
                criteria: ALL_SELECT_CRITERIA[rng.gen_range(0..ALL_SELECT_CRITERIA.len())],
                min: low,
                max: low + rng.gen_range(40..200u64),
            },
        };

        // Leave out "Guide"
        self.sorter.criteria = ALL_CRITERIA[rng.gen_range(0..ALL_CRITERIA.len() - 1)];
        // Mostly real sorting, sometimes one of the effects
        self.sorter.algorithm = match rng.gen_range(0..10u32) {
            0..=4 => SortingAlgorithm::Mapsort,
            5 => SortingAlgorithm::Shellsort,
            6 => SortingAlgorithm::Glitchsort,
            7 => SortingAlgorithm::Partial,
            8 => SortingAlgorithm::Stretch,
            _ => SortingAlgorithm::Shift,
        };
        self.sorter.passes = rng.gen_range(2..60u32);
        self.sorter.amount = (rng.gen_range(0.2..0.8f32) * 100.0).round() / 100.0;
        self.alternate = rng.gen_bool(0.2);
        self.channels = if rng.gen_bool(0.8) { ChannelMode::All } else { ALL_CHANNELS[rng.gen_range(1..4usize)] };
        self.posterize = if rng.gen_bool(0.85) { 0 } else { rng.gen_range(2..8u8) };
    }
}
