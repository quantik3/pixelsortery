<div align="center" style="text-align: center;">
    <a target="_blank" rel="noopener noreferrer" href="assets/logo.png" style="text-align: center;">
        <img src="assets/logo.png" alt="Pixelsortery-Logo" style="width: 50%;">
    </a>
</div>

# Pixlflow - A Pixelsorter App written in Rust!

> **Pixlflow is a fork** of [Pixelsortery by Lxtharia](https://github.com/Lxtharia/pixelsortery), maintained by [quantik3](https://github.com/quantik3).
> It adds faster sorting, painted masks, more paths and effects, blend modes, presets, animation export and more.
> See [What's new in this fork](#whats-new-in-this-fork) below. The rest of this README is based on the original project.

**Authors:** [Lxtharia](https://github.com/Lxtharia) (original author), [quantik3](https://github.com/quantik3) (fork)

Inspired by [ASDFPixelSort](https://github.com/kimasendorf/ASDFPixelSort) by [Kim Asendorf](https://kimasendorf.com/)!
I've written a few Pixelsorters before:
- My first one originated from modifying the original processing code until it became a little Java Swing Application: [Pixel-Sorter-App](https://github.com/Lxtharia/pixel-sorter-app).
- My second implementation was in C. It may or may not have been faster and the code was a mess, but it was usable from the command line!
- But now I have engineered THIS pixelsorter with _far greater goals_, created as a rust practice project but striving to be BEST and FASTEST FOSS-Pixelsorter of them all (maybe)!!!

## Showcase


https://github.com/user-attachments/assets/95c1d76e-752e-46b3-9ed4-d9da214b1547


## What's new in this fork

### Faster
- The sorting works on a contiguous copy of every path instead of pointers scattered over the image: about 1.5-2.6x faster on the same settings, with exactly the same result
- Mapsort is a real counting sort now, and a threshold that selects every pixel is skipped
- **Background sorting**: the window stays responsive while sorting, only the newest settings get sorted
- **Fast preview**: while a slider is dragged, a smaller copy is sorted; the full image follows when you let go

### Masks and brushes
- **Paint a mask** with the brush (`B`): only the painted area is sorted. Left click paints, right click erases
- Soft brush (**Hardness**), fill / empty / invert / remove, **save and load** masks as images, **mask from threshold**
- **Direction brush**: paint strokes on the image and the sorting follows them (path *Drawn*), the directions blend smoothly between the strokes

### Paths
- **Waves**: vertical or horizontal wavy lines (amplitude, wavelength)
- **Flow Field**: lines that follow a swirling noise field (swirl size, seed)
- **Star**: star shaped rings (points, inner radius), or any regular polygon
- **Movable center** for rays, circles, spirals and stars: sliders or *Pick on image*

### Selecting and sorting
- **Edges** selector: spans end where the color changes a lot, so objects stay recognizable
- **Combined** selector: two thresholds joined with AND / OR, e.g. only pixels that are bright *and* red
- New criteria: **Lightness** (perceived, CIE L*), **Warmth**, **Red / Green / Blue**, and **Guide**: sort by the brightness of a second image
- New algorithms: **Partial** (only a few passes, "dripping" look), **Stretch** ("melting"), **Shift** (block glitch)
- **Alternate direction** (herringbone), move only one **color channel**, **posterize** the sorted pixels

### Layers
- **Blend modes** (Normal, Lighten, Darken, Multiply, Screen, Overlay, Difference) and **opacity** per layer
- Every layer has its own mask, guide image and painted directions

### Comfort
- **Undo / Redo** (`Ctrl+Z`, `Ctrl+Y`), **zoom** (mouse wheel) and **pan** (middle mouse button), `F` fits the image
- **Compare**: original and sorted image side by side with a movable divider
- Open images with the button or **drag and drop** them into the window
- **Presets**: save settings under a name, apply them later, copy them as text; **Random** settings for happy accidents
  (presets are stored in `presets.txt` next to the program, so you can copy the folder together with them)

### Animation and video
- **Animation**: set the start and the end settings of a layer, export a looping **GIF** or **PNG frames**
- **Edit a video frame with layers**: use all tools (masks, layers, directions) on one frame, then export the whole video with them, optionally animated from the start to the end settings over the video

### New command line options
```bash
# Sort along waves / a flow field / star shaped rings, with a center
pixlflow -i in.png -o out.png --waves 20 120
pixlflow -i in.png -o out.png --flow 150 1
pixlflow -i in.png -o out.png --star 5 0.5 --center 0.3 0.6

# End spans at edges, sort by lightness, herringbone pattern
pixlflow -i in.png -o out.png --down --edges 80 --lightness --alternate

# Only sort pixels that are red AND bright
pixlflow -i in.png -o out.png --down --combine and hue:0:30 bright:100:255

# Only sort where mask.png is white, sort by the brightness of another image
pixlflow -i in.png -o out.png --mask mask.png --guide other.png

# Effects: partial sort, one color channel, posterize
pixlflow -i in.png -o out.png --partial 10 --channel r --posterize 4

# Settings copied from the GUI (Presets -> Copy as text)
pixlflow -i in.png -o out.png --preset "path=down;selector=edges:80;criteria=Lightness"
```
See `pixlflow --help` for all options.

## Installation

The original project has binaries on its [Releases page](https://github.com/Lxtharia/pixelsortery/releases).
For this fork, the Windows build is created by GitHub Actions: open the *Actions* tab, choose the latest *Build Windows* run and download the zip from *Artifacts* (keep the exe and the DLLs in one folder).

### Building from source
Ensure rust and cargo are installed, then type this on the command line:
```bash
git clone https://github.com/quantik3/pixelsortery
cd pixelsortery
cargo install --path .
```
Video support is optional, it needs the FFmpeg development files and libclang:
```bash
cargo install --path . --features video
```

## Usage
### GUI Usage
To start the gui, double-click it or run the binary from the command line without any arguments:
```bash
/path/to/pixlflow.exe
```

You can also pass the --gui flag on the command line to apply other options when opening the GUI.
```bash
pixlflow --gui --input image.png --diagonal 30
```
This would for example open the gui with the image loaded and the direction set to `diagonal` with a 30° angle

### CLI Usage
You can also sort images via the command line.\
This is useful if want to sort many images at once or create animations by manipulating a parameter via code.\
There are a bunch of different options available which I will not add to the README, as the `--help` output is way more up-to-date and nicely formatted, so go look at that.

1. Look at the examples to understand how to use it
2. Look at the help to see all available options
```bash
pixlflow --help
```
2. Read the explanation below to understand what pixelsorting actually is (if you want to)
4. Start playing around with different parameters!

Some examples:
```bash
# Sorts pixels downwards
pixlflow -i input.png -o output.png --down

# Sort diagonally (in a 30° angle), only sort orange pixels (hue:10:40) and use the glitchsort algorithm 
pixlflow -i image.png --diagonal 30 --thres hue:10:40 --glitchsort --output sortedimg.png

# You can also chain commands by reading and writing to stdin/stdout (by setting the filename to `-`)
pixlflow -i image.png --diagonal 30 --output - | pixlflow --input - --hilbert --glitchsort --output doublesortedimg.png

# Sort the same image a hundred times, creating animation frames
for i in $(seq 1 100); do
    echo "Sorting $i"
    pixlflow -i input.png -o "output-$i.png" --thres "bright:0:$i" --down
done
```

### Video Sorting
Pixlflow has the ability to sort videos!
In the **GUI**, just open a video and the videoplayer will be shown.
The video will be autosorted by default, this can lead to a slow playback with bigger videos.
You can also pause the video and adjust the sorting parameters with less lag.
Note that this is only a preview. When you click on "Start export", a separate process will start and sort the video with the last set parameters.

On the **CLI**, you simply need to specify the path to the input file and the output file with a video extension.
You can also open a video input and sort the video *live*. You need to set the output path to `-` to display the sorted live feed.
For example:
```bash
pixlflow --input /dev/video0 --diagonal 30 --output - 
```

## Features

- [x] Choose to sort lines in all 4 directions
- [x] Sort pixels _diagonally_, in rectangular shape, in circles, 
- [x] Sort pixels in the shape of the space filling hilbert curve (very sweet)
- [x] Choose to select spans of random or fixed length
- [x] Choose to select spans based on their hue/brightness/saturation
- [x] Sorts pixel with MAPSORT (HELL YEAH!!)
- [x] Also allows to use a COMPLETELY BROKEN sorting algorithm, creating a super rad glitch effect (try it out)
- [x] A super cool CLI interface!
- [x] Read input from stdin and write to stdout, allowing easy chaining
- [x] a GUI (holy damn)
- [x] a really good GUI (damn holy)
- [x] Sort _multiple_ times with layers (wowie)
- [x] Sort VIDEOS (this is getting crazy!)

### Planned Features

- [x] a godlike GUI where you can edit masks in real time *(fork)*
- [x] Add more pattern patterns to sort by (sin waves, star shape, ...) *(fork)*
- [ ] Allow to read custom pathing data from file (in the fork, paths can be painted with the direction brush)
- [x] Allow to use MASKS to control which areas should be sorted and which shouldn't *(fork)*
- [x] Allow to logically combine selectors (sort pixels that are bright _and_ red) *(fork)*
- [x] Built-in animation (someday maybe) *(fork)*
- [x] Animation for sorting videos (another someday maybe) *(fork)*

## Explanation (How it works)

The pixelsorter is quite modular to allow for easy extension in the future.
There are three main stages in the sorting process:

1. **Creating Paths**
    - Usually determines the sorting "direction", but can be much more
    - Defines the order in which pixels are processed
    - Allows to sort in horizontal or vertical lines
    - Allows to sort in different shapes like spirals
    - Allows to sort in a space filling fractal, the [hilbert curve](https://en.wikipedia.org/wiki/Hilbert_curve)
        - Gilbert algorithm taken from [jakubcerveny](https://github.com/jakubcerveny/gilbert)
2. **Selecting**
    - The magic behind the cool pixelsort-effect: we only sort _intervals_ of pixels (lets call them "spans")
    - We are iterating pixels in whatever order the iteration has yielded them
    - We select these intervals by grouping similar pixels together
    - Examples:
        - Group (select) bright pixels (pixels with their brightness value between 180 and 255)
        - Select blue pixels (pixels with their hue value between 190 and 240)
    - All other pixels, not fulfilling this requirement, will not get sorted
    - We only group pixels that come after each other, so (D, B, B, D, D, B, D) gives us the spans (B, B) and (B) 
        - With B being a "bright" pixel and D being a "dark" one, following the previous example
3. **Sorting**
    - Finally, sorting
    - You can sort by different criteria
    - Usually sorting by Brightness looks the most smooth
    - You can also change the sorting algorithm, which mostly has impact on performance, but could - in theory - create different results (see [Sorting algorithm stability](https://en.wikipedia.org/wiki/Sorting_algorithm#Stability))

<details>
<summary>
<h3> ~=~=~=~=~= A COOL CHART, CLICK HERE, LOOK AT IT =~=~=~=~=~ </h3>
</summary>

![Pixelsortery-Chart](assets/Pixelsortery-Chart.png)
it will get better hopefully, maybe, eventually

</details>


## Cool!

If you like this piece of software, consider buying the original author a coffee! :D

<a href='https://ko-fi.com/V7V117RIM6' target='_blank'><img height='36' style='border:0px;height:36px;' src='https://storage.ko-fi.com/cdn/kofi4.png?v=6' border='0' alt='Buy Me a Coffee at ko-fi.com' /></a>

## Yeah
yeah
```
Yeah!!
```
> yeah

| yeah |
| ---- |
| _yeah_ |
