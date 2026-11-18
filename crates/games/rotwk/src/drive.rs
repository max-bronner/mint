use std::path::PathBuf;
use std::time::{Duration, Instant};

use mint_app::{
    App, FixedTimestep, FrameLog, FrameMarks, InputEvent, Key, PointerButton, SIMULATION_HZ,
};
use mint_render::Renderer;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    Wait(u32),
    Ticks(u64),
    Speed(f64),
    Move(f32, f32),
    Click(f32, f32, PointerButton),
    ClickHere(PointerButton),
    Down(f32, f32, PointerButton),
    Up(PointerButton),
    Key(Key),
    Ctrl(bool),
    Shift(bool),
    Alt(bool),
    Scroll(f32),
    Select(String),
    Do(String),
    Try(String, String),
    Shot(PathBuf),
    Time(u32),
    Resize(u32, u32),
    Real(u32),
}

pub(crate) fn parse(script: &str) -> Result<Vec<Step>, String> {
    script
        .lines()
        .map(|line| line.split_once('#').map_or(line, |(code, _)| code))
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_step)
        .collect()
}

fn parse_step(step: &str) -> Result<Step, String> {
    let (verb, rest) = step.split_once(' ').unwrap_or((step, ""));
    let rest = rest.trim();
    let point = || -> Result<(f32, f32), String> {
        let (x, y) = rest
            .split_once(',')
            .ok_or_else(|| format!("--drive: {step:?} wants X,Y"))?;
        let number = |s: &str| {
            s.trim()
                .parse::<f32>()
                .map_err(|_| format!("--drive: {step:?} wants numbers"))
        };
        Ok((number(x)?, number(y)?))
    };
    let count = || {
        rest.parse::<u32>()
            .map_err(|_| format!("--drive: {step:?} wants a frame count"))
    };
    match verb {
        "wait" => Ok(Step::Wait(count()?)),
        "speed" if rest == "max" => Ok(Step::Speed(MAX_SPEED)),
        "speed" => rest
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
            .map(Step::Speed)
            .ok_or_else(|| format!("--drive: {step:?} wants a positive factor or max")),
        "ticks" => rest
            .parse::<u64>()
            .map(Step::Ticks)
            .map_err(|_| format!("--drive: {step:?} wants a tick count")),
        "time" => Ok(Step::Time(count()?)),
        "real" => Ok(Step::Real(count()?)),
        "resize" => {
            let (w, h) = point()?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(Step::Resize(w.max(1.0) as u32, h.max(1.0) as u32))
        }
        "move" => point().map(|(x, y)| Step::Move(x, y)),
        "click" if rest == "here" => Ok(Step::ClickHere(PointerButton::Primary)),
        "rclick" if rest == "here" => Ok(Step::ClickHere(PointerButton::Secondary)),
        "mclick" if rest == "here" => Ok(Step::ClickHere(PointerButton::Middle)),
        "click" => point().map(|(x, y)| Step::Click(x, y, PointerButton::Primary)),
        "rclick" => point().map(|(x, y)| Step::Click(x, y, PointerButton::Secondary)),
        "mclick" => point().map(|(x, y)| Step::Click(x, y, PointerButton::Middle)),
        "down" => {
            let (xy, letter) = rest
                .rsplit_once(' ')
                .ok_or_else(|| format!("--drive: {step:?} wants X,Y l|r|m"))?;
            let button =
                parse_button(letter).ok_or_else(|| format!("--drive: {step:?} wants l|r|m"))?;
            let (x, y) = xy
                .split_once(',')
                .ok_or_else(|| format!("--drive: {step:?} wants X,Y l|r|m"))
                .and_then(|(x, y)| {
                    let number = |s: &str| {
                        s.trim()
                            .parse::<f32>()
                            .map_err(|_| format!("--drive: {step:?} wants numbers"))
                    };
                    Ok((number(x)?, number(y)?))
                })?;
            Ok(Step::Down(x, y, button))
        }
        "up" => parse_button(rest)
            .map(Step::Up)
            .ok_or_else(|| format!("--drive: {step:?} wants l|r|m")),
        "scroll" => rest
            .parse::<f32>()
            .map(Step::Scroll)
            .map_err(|_| format!("--drive: {step:?} wants a number of lines")),
        "key" => parse_key(rest)
            .map(Step::Key)
            .ok_or_else(|| format!("--drive: {step:?} names no key")),
        "ctrl" if rest == "on" => Ok(Step::Ctrl(true)),
        "ctrl" if rest == "off" => Ok(Step::Ctrl(false)),
        "shift" if rest == "on" => Ok(Step::Shift(true)),
        "shift" if rest == "off" => Ok(Step::Shift(false)),
        "alt" if rest == "on" => Ok(Step::Alt(true)),
        "alt" if rest == "off" => Ok(Step::Alt(false)),
        "select" if !rest.is_empty() => Ok(Step::Select(rest.to_owned())),
        "do" if !rest.is_empty() => Ok(Step::Do(rest.to_owned())),
        "try" => match rest.split_once(' ') {
            Some((inner @ ("select" | "do"), argument)) if !argument.trim().is_empty() => {
                Ok(Step::Try(inner.to_owned(), argument.trim().to_owned()))
            }
            _ => Err(format!("--drive: {step:?} wants try select|do ...")),
        },
        "shot" if !rest.is_empty() => Ok(Step::Shot(PathBuf::from(rest))),
        _ => Err(format!("--drive: {step:?} is not a step")),
    }
}

fn parse_button(letter: &str) -> Option<PointerButton> {
    match letter {
        "l" => Some(PointerButton::Primary),
        "r" => Some(PointerButton::Secondary),
        "m" => Some(PointerButton::Middle),
        _ => None,
    }
}

fn parse_key(name: &str) -> Option<Key> {
    match name {
        "esc" => Some(Key::Escape),
        "space" => Some(Key::Space),
        "enter" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        _ => {
            if let Some(digit) = name.strip_prefix('f') {
                if let Some(n) = digit.parse::<u8>().ok().filter(|n| (1..=12).contains(n)) {
                    return Some(Key::Function(n));
                }
            }
            if let Some(rest) = name.strip_prefix("ctrl+shift+") {
                if let Some(digit) = rest.parse::<u8>().ok().filter(|digit| *digit <= 9) {
                    return Some(Key::CtrlShiftDigit(digit));
                }
                let mut chars = rest.chars();
                return match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(Key::CtrlShiftCharacter(c.to_ascii_lowercase())),
                    _ => None,
                };
            }
            let (ctrl, name) = match name.strip_prefix("ctrl+") {
                Some(rest) => (true, rest),
                None => (false, name),
            };
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if ctrl => Some(Key::CtrlCharacter(c.to_ascii_lowercase())),
                (Some(c), None) => Some(Key::Character(c.to_ascii_lowercase())),
                _ => None,
            }
        }
    }
}

const MAX_SPEED: f64 = 1000.0;

const TICK_STALL_FRAMES: u32 = 200_000;

pub(crate) struct Driver {
    clock: FixedTimestep,
    dt: Duration,
    frames: u64,
    exited: bool,
    speed: Option<f64>,
    running_speed: Option<f64>,
    log: FrameLog,
}

impl Driver {
    pub(crate) fn new(dt: Duration) -> Self {
        Self {
            clock: FixedTimestep::new(SIMULATION_HZ),
            dt,
            frames: 0,
            exited: false,
            speed: None,
            running_speed: None,
            log: FrameLog::from_env(),
        }
    }

    pub(crate) fn wait_ticks<A: App>(
        &mut self,
        app: &mut A,
        renderer: &mut Renderer,
        target: u64,
        tick: &impl Fn(&A) -> Option<u64>,
    ) -> Result<(), String> {
        let mut last = tick(app);
        let mut idle = 0u32;
        self.running_speed = self.speed;
        let reached = |app: &A| tick(app).is_some_and(|now| now >= target);
        while !self.exited && last.is_none_or(|now| now < target) {
            self.frame_until(app, renderer, &reached)?;
            let now = tick(app);
            idle = if now == last {
                idle.saturating_add(1)
            } else {
                0
            };
            if idle >= TICK_STALL_FRAMES {
                return Err(format!(
                    "--drive: ticks {target}: no tick in {idle} frame(s), at {now:?}"
                ));
            }
            last = now;
        }
        self.running_speed = None;
        self.clock = FixedTimestep::new(SIMULATION_HZ);
        Ok(())
    }

    pub(crate) fn frame<A: App>(
        &mut self,
        app: &mut A,
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        self.frame_until(app, renderer, &|_| false)
    }

    fn frame_until<A: App>(
        &mut self,
        app: &mut A,
        renderer: &mut Renderer,
        until: &dyn Fn(&A) -> bool,
    ) -> Result<(), String> {
        if self.exited {
            return Ok(());
        }
        let steps = self.clock.advance_unless_paused(
            self.dt,
            self.running_speed.unwrap_or_else(|| app.speed()),
            app.paused(),
        );
        let start = Instant::now();
        let already = self.clock.steps_taken();
        for step in 0..u64::from(steps) {
            if until(app) {
                break;
            }
            app.update(
                already
                    .saturating_sub(u64::from(steps))
                    .saturating_add(step),
            );
        }
        let stepped = Instant::now();
        app.frame(self.dt.as_secs_f32());
        if app.paused() {
            self.clock = FixedTimestep::new(SIMULATION_HZ);
        }
        let framed = Instant::now();
        let blend = self.clock.blend();
        self.frames = self.frames.saturating_add(1);
        let drawn = app.draw(renderer, blend);
        let marks = FrameMarks {
            start,
            stepped,
            framed,
            drawn: Instant::now(),
        };
        self.log
            .record(self.dt, steps, &marks, renderer.present_wait(), &*app);
        let drawn = drawn.map_err(|error| {
            let mut text = error.to_string();
            let mut cause = std::error::Error::source(&error);
            while let Some(source) = cause {
                text.push_str(&format!(
                    "
  caused by: {source}"
                ));
                cause = source.source();
            }
            text
        });
        let _ = app.cursor();
        if app.exit_requested() {
            self.exited = true;
            println!("drive: the app asked to exit at frame {}", self.frames);
        }
        drawn
    }
}

pub(crate) fn run<A: App>(
    app: &mut A,
    renderer: &mut Renderer,
    mut size: (u32, u32),
    dt: Duration,
    steps: &[Step],
    mut hook: impl FnMut(&mut A, &str, &str) -> Result<String, String>,
    tick: impl Fn(&A) -> Option<u64>,
) -> Result<(), String> {
    let mut driver = Driver::new(dt);
    app.input(&InputEvent::Resized {
        width: size.0,
        height: size.1,
    });
    for step in steps {
        match step {
            Step::Wait(n) => {
                for _ in 0..*n {
                    driver.frame(app, renderer)?;
                }
            }
            Step::Speed(factor) => {
                driver.speed = Some(*factor);
                println!("drive: speed {factor}x");
            }
            Step::Ticks(target) => driver.wait_ticks(app, renderer, *target, &tick)?,
            Step::Move(x, y) => {
                app.input(&InputEvent::PointerMoved { x: *x, y: *y });
                driver.frame(app, renderer)?;
            }
            Step::Click(x, y, button) => {
                app.input(&InputEvent::PointerMoved { x: *x, y: *y });
                driver.frame(app, renderer)?;
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: true,
                });
                driver.frame(app, renderer)?;
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: false,
                });
                driver.frame(app, renderer)?;
            }
            Step::ClickHere(button) => {
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: true,
                });
                driver.frame(app, renderer)?;
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: false,
                });
                driver.frame(app, renderer)?;
            }
            Step::Down(x, y, button) => {
                app.input(&InputEvent::PointerMoved { x: *x, y: *y });
                driver.frame(app, renderer)?;
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: true,
                });
                driver.frame(app, renderer)?;
            }
            Step::Up(button) => {
                app.input(&InputEvent::PointerButton {
                    button: *button,
                    pressed: false,
                });
                driver.frame(app, renderer)?;
            }
            Step::Key(key) => {
                app.input(&InputEvent::Key {
                    key: *key,
                    pressed: true,
                });
                driver.frame(app, renderer)?;
                app.input(&InputEvent::Key {
                    key: *key,
                    pressed: false,
                });
                driver.frame(app, renderer)?;
            }
            Step::Ctrl(held) => {
                app.input(&InputEvent::Modifiers { ctrl: *held });
                driver.frame(app, renderer)?;
            }
            Step::Shift(held) => {
                app.input(&InputEvent::Key {
                    key: Key::Shift,
                    pressed: *held,
                });
                driver.frame(app, renderer)?;
            }
            Step::Alt(held) => {
                app.input(&InputEvent::Key {
                    key: Key::Alt,
                    pressed: *held,
                });
                driver.frame(app, renderer)?;
            }
            Step::Scroll(lines) => {
                app.input(&InputEvent::Scroll { lines: *lines });
                driver.frame(app, renderer)?;
            }
            Step::Select(which) => {
                let said = hook(app, "select", which)?;
                println!("drive: select {which} - {said}");
                driver.frame(app, renderer)?;
            }
            Step::Do(action) => {
                let said = hook(app, "do", action)?;
                println!("drive: do {action} - {said}");
                driver.frame(app, renderer)?;
            }
            Step::Try(verb, argument) => {
                match hook(app, verb, argument) {
                    Ok(said) => println!("drive: {verb} {argument} - {said}"),
                    Err(error) => println!("drive: try {verb} {argument} - skipped: {error}"),
                }
                driver.frame(app, renderer)?;
            }
            Step::Shot(path) => {
                let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
                crate::game::write_tga(path, size.0, size.1, &pixels).map_err(|e| e.to_string())?;
                println!(
                    "drive: frame {} written to {}",
                    driver.frames,
                    path.display()
                );
            }
            Step::Resize(width, height) => {
                renderer.resize(*width, *height);
                size = renderer.size();
                app.input(&InputEvent::Resized {
                    width: size.0,
                    height: size.1,
                });
                println!("drive: resized to {}x{}", size.0, size.1);
                driver.frame(app, renderer)?;
            }
            Step::Real(ms) => {
                let until = Instant::now() + Duration::from_millis(u64::from(*ms));
                while Instant::now() < until && !driver.exited {
                    let began = Instant::now();
                    driver.frame(app, renderer)?;
                    if let Some(rest) = dt.checked_sub(began.elapsed()) {
                        std::thread::sleep(rest);
                    }
                }
            }
            Step::Time(n) => {
                let mut costs = Vec::new();
                for _ in 0..*n {
                    let started = Instant::now();
                    driver.frame(app, renderer)?;
                    costs.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                costs.sort_by(f64::total_cmp);
                if let (Some(first), Some(last)) = (costs.first(), costs.last()) {
                    #[allow(clippy::cast_precision_loss)]
                    let mean = costs.iter().sum::<f64>() / costs.len() as f64;
                    println!(
                        "drive: {n} frame(s), whole frame (update, frame, draw, GPU submit) \
                         min {first:.3} / mean {mean:.3} / max {last:.3} ms"
                    );
                }
            }
        }
        if driver.exited {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_is_steps_and_a_bad_step_is_an_error() {
        let steps = parse("wait 3; click 10,20 ;rclick 1.5,2; key c; key esc; key down; scroll -2; select producer; do buy SCIENCE_Heal; shot a.tga; time 4")
            .expect("parses");
        assert_eq!(
            steps,
            vec![
                Step::Wait(3),
                Step::Click(10.0, 20.0, PointerButton::Primary),
                Step::Click(1.5, 2.0, PointerButton::Secondary),
                Step::Key(Key::Character('c')),
                Step::Key(Key::Escape),
                Step::Key(Key::ArrowDown),
                Step::Scroll(-2.0),
                Step::Select("producer".to_owned()),
                Step::Do("buy SCIENCE_Heal".to_owned()),
                Step::Shot(PathBuf::from("a.tga")),
                Step::Time(4),
            ]
        );
        assert!(parse("wait x").is_err());
        assert!(parse("click 10").is_err());
        assert!(parse("jump 1").is_err());
        assert!(parse("key ab").is_err());
        assert!(parse("do").is_err(), "an action names what to do");
        assert_eq!(
            parse("# intro\nwait 1 # skip\r\nwait 2; wait 3").expect("parses"),
            vec![Step::Wait(1), Step::Wait(2), Step::Wait(3)]
        );
        assert_eq!(
            parse("try do attack named A B").expect("parses"),
            vec![Step::Try("do".to_owned(), "attack named A B".to_owned())]
        );
        assert!(
            parse("try key a").is_err(),
            "only select and do can be tried"
        );
    }

    #[test]
    fn a_press_can_be_held_without_its_release() {
        let steps = parse("down 1,2 r; move 3,4; up r").expect("parses");
        assert_eq!(
            steps,
            vec![
                Step::Down(1.0, 2.0, PointerButton::Secondary),
                Step::Move(3.0, 4.0),
                Step::Up(PointerButton::Secondary),
            ]
        );
    }

    #[test]
    fn down_m_is_the_middle_button() {
        assert_eq!(
            parse("down 5,6 m; up m").expect("parses"),
            vec![
                Step::Down(5.0, 6.0, PointerButton::Middle),
                Step::Up(PointerButton::Middle),
            ]
        );
        assert!(parse("down 1,2 x").is_err());
        assert!(parse("up x").is_err());
    }

    #[test]
    fn ctrl_shift_plus_a_letter_is_a_ctrl_shift_character() {
        assert_eq!(
            parse("key ctrl+shift+l").expect("parses"),
            vec![Step::Key(Key::CtrlShiftCharacter('l'))]
        );
        assert_eq!(
            parse("key ctrl+shift+3").expect("parses"),
            vec![Step::Key(Key::CtrlShiftDigit(3))]
        );
        assert!(parse("key ctrl+shift+ll").is_err());
    }

    struct Loading {
        load_frames: u32,
        frames: u32,
        ticks: u64,
    }

    impl App for Loading {
        fn input(&mut self, _event: &InputEvent) {}
        fn update(&mut self, _step: u64) {
            if self.frames >= self.load_frames {
                self.ticks += 1;
            }
        }
        fn frame(&mut self, _dt: f32) {
            self.frames += 1;
        }
        fn draw(&mut self, _renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
            Ok(())
        }
    }

    fn tick_of_do(load_frames: u32, dt_ms: u64, speed: &str) -> Option<u64> {
        let mut renderer = match Renderer::offscreen(4, 4) {
            Ok(renderer) => renderer,
            Err(mint_render::Error::NoAdapter) => {
                eprintln!("SKIPPED: no graphics adapter on this machine");
                return None;
            }
            Err(error) => panic!("offscreen renderer: {error}"),
        };
        let mut app = Loading {
            load_frames,
            frames: 0,
            ticks: 0,
        };
        let mut seen = Vec::new();
        run(
            &mut app,
            &mut renderer,
            (4, 4),
            Duration::from_millis(dt_ms),
            &parse(&format!(
                "speed {speed}; ticks 10; do flag; key a; do flag; ticks 25"
            ))
            .expect("parses"),
            |app, _, _| {
                seen.push(app.ticks);
                Ok(String::new())
            },
            |app| (app.frames >= app.load_frames).then_some(app.ticks),
        )
        .expect("runs");
        assert_eq!(app.ticks, 25);
        seen.first().copied()
    }

    struct Tail {
        load_frames: u32,
        blocked_frames: u32,
        frames: u32,
        blends: Vec<f32>,
    }

    impl App for Tail {
        fn input(&mut self, _event: &InputEvent) {}
        fn update(&mut self, _step: u64) {}
        fn frame(&mut self, _dt: f32) {
            self.frames += 1;
        }
        fn draw(&mut self, _renderer: &mut Renderer, blend: f32) -> mint_render::Result<()> {
            self.blends.push(blend);
            Ok(())
        }
        fn paused(&self) -> bool {
            self.frames > self.load_frames && self.frames <= self.load_frames + self.blocked_frames
        }
    }

    fn blends_after_load(load_frames: u32, blocked_frames: u32) -> Option<Vec<f32>> {
        let mut renderer = match Renderer::offscreen(4, 4) {
            Ok(renderer) => renderer,
            Err(mint_render::Error::NoAdapter) => {
                eprintln!("SKIPPED: no graphics adapter on this machine");
                return None;
            }
            Err(error) => panic!("offscreen renderer: {error}"),
        };
        let mut app = Tail {
            load_frames,
            blocked_frames,
            frames: 0,
            blends: Vec::new(),
        };
        let mut driver = Driver::new(Duration::from_micros(16_667));
        for _ in 0..load_frames + blocked_frames + 6 {
            driver.frame(&mut app, &mut renderer).expect("draws");
        }
        Some(app.blends.split_off(load_frames as usize))
    }

    #[test]
    fn the_clock_phase_after_a_blocked_tail_does_not_depend_on_the_frames_before_it() {
        let runs = [(3, 3), (4, 4), (41, 3), (100, 4)].map(|(load, blocked)| {
            blends_after_load(load, blocked).map(|blends| {
                let (tail, running) = blends.split_at(blocked as usize);
                assert!(tail.iter().all(|blend| *blend == 0.0), "{tail:?}");
                running.to_vec()
            })
        });
        if runs.iter().any(Option::is_none) {
            return;
        }
        assert!(runs.iter().all(|run| *run == runs[0]), "{runs:?}");
    }

    #[test]
    fn a_step_after_ticks_lands_on_the_same_tick_whatever_the_frame_pacing() {
        let runs = [
            (3, 17, "1"),
            (40, 17, "1"),
            (7, 9, "1"),
            (5, 17, "max"),
            (30, 9, "3"),
        ]
        .map(|(load, dt, speed)| tick_of_do(load, dt, speed));
        if runs.iter().any(Option::is_none) {
            return;
        }
        assert_eq!(runs, [Some(10); 5]);
    }
}
