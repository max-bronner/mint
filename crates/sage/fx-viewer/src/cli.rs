use std::path::PathBuf;

use crate::tree::Category;

#[derive(Debug, Default)]
pub struct Options {
    pub installs: Vec<PathBuf>,
    pub tree: Option<String>,
    pub list: Option<(Category, String)>,
    pub select: Option<String>,
    pub play: Option<String>,
    pub row: Option<String>,
    pub frames: u32,
    pub shot: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub help: bool,
}

pub const USAGE: &str = "\
mint FX viewer - one composed effect on screen, with the tree it is built from

  cargo run -p mint-sage-fx-viewer -- --install <path> [options]

  --install <path>       a retail installation to mount; repeat for more,
                         base game first (a later mount shadows an earlier)
  --select <name>        select this FXList, particle system, object or
                         weapon at startup
  --size <w>x<h>         window size (default 1600x900)

Windowless, for checking the data without opening anything:

  --tree <name>          print the expanded tree of one FXList, particle
                         system, object or weapon, with the role of every edge
  --list <category> [f]  print the roots of a category - fx-lists, systems,
                         objects or weapons - narrowed by an optional
                         substring
  --play <name>          no window: fire this root offscreen and print the
                         status line every 15 frames - what runs, not how it
                         looks
  --row <text>           with --play: the first row under the root whose line
                         contains this text, instead of the root
  --frames <n>           with --play: simulation frames to run (default 90)
  --shot <path>          with --play: write the last frame as a TGA - proof
                         that something reached the picture, not of its look
  --help                 this text";

pub const KEYS: &str = "--- fx-viewer controls ---
camera (the world viewer's)
  left-drag      move: the scene follows the pointer, in the screen's plane
  right-drag     rotate around the point looked at (pitch clamped)
  wheel          over the 3D view: zoom; over the sidebar: scroll
  W/A/S/D        forward/left/back/right along the ground
  Q/E            down/up
  Shift          move faster while held
  R              reset - frame the origin again
playback
  click a row    select it AND play it
  Space          replay the selected row
  P              pause (Ctrl+P too)
  N              pause and step one simulation frame (Ctrl+N too)
  1 2 3 4 5      speed 0.1x / 0.25x / 0.5x / 1x / 2x
  L              loop: replay when the last system ends
  X              clear every effect
  M              show or hide the object's model
  G              ground grid
  H              the control box (its buttons are these keys, clickable)
target marker (the list's secondary point, what SetTargetMatrix aims at)
  [ / ]          nearer / further
  , / .          bearing left / right
tree
  click a row    select, play, and expand or collapse
  blue dot       at the row's right end: hide this row's layer
  amber dot      solo this row's layer
  Ctrl+H         show everything again
  Ctrl+J         clear the solo
  Ctrl+T         print the selected subtree, paste-ready
  Ctrl+K         this list
--------------------------";

pub fn parse_arguments() -> Result<Options, String> {
    parse_from(std::env::args().skip(1))
}

pub fn parse_from(arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        width: 1600,
        height: 900,
        frames: 90,
        ..Options::default()
    };
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "--install" => options.installs.push(PathBuf::from(value("--install")?)),
            "--tree" => options.tree = Some(value("--tree")?),
            "--select" => options.select = Some(value("--select")?),
            "--play" => options.play = Some(value("--play")?),
            "--row" => options.row = Some(value("--row")?),
            "--shot" => options.shot = Some(PathBuf::from(value("--shot")?)),
            "--frames" => {
                let text = value("--frames")?;
                options.frames = text
                    .parse()
                    .map_err(|_| format!("--frames is not a number: {text}"))?;
            }
            "--list" => {
                let text = value("--list")?;
                let category = Category::parse(&text).ok_or_else(|| {
                    format!("--list wants fx-lists, systems, objects or weapons, got {text}")
                })?;
                let filter = match arguments.peek() {
                    Some(next) if !next.starts_with("--") => arguments.next(),
                    _ => None,
                };
                options.list = Some((category, filter.unwrap_or_default()));
            }
            "--size" => {
                let text = value("--size")?;
                let (width, height) = text
                    .split_once('x')
                    .ok_or_else(|| format!("--size wants <w>x<h>, got {text}"))?;
                options.width = width
                    .parse()
                    .map_err(|_| format!("--size width is not a number: {width}"))?;
                options.height = height
                    .parse()
                    .map_err(|_| format!("--size height is not a number: {height}"))?;
            }
            "--help" | "-h" => options.help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn list_takes_an_optional_filter_without_eating_the_next_flag() {
        let options = parse_from(
            ["--list", "systems", "--help"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(options.list, Some((Category::Systems, String::new())));
        assert!(options.help);

        let options = parse_from(
            ["--list", "fx-lists", "gandalf"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(
            options.list,
            Some((Category::FxLists, "gandalf".to_owned()))
        );
    }
}
