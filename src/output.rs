//! `pleamar-wm output NAME …`: a monitor changed while the session runs, and
//! the answer, what was done or why not. It is said on `output.sock`, next to
//! the session's other sockets (`PLEAMAR_SOCKETS`), and the session does it in
//! its `apply_output`, where the card is.
//!
//! A wish is checked whole before any of it is done: one part that cannot be
//! done leaves the monitor as it was. Only what needs no new mode is done for
//! now (where it is, variable refresh); a mode, a scale or a turn needs the
//! monitor's buffers made again, and is answered as not yet.

use crate::config::ModeWish;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// What a monitor is asked to become: each part said is a change, each part
/// left out stays as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wish {
    pub mode: Option<ModeWish>,
    pub scale: Option<f64>,
    /// Quarter turns, as `transform` on a monitor line.
    pub transform: Option<u8>,
    pub vrr: Option<bool>,
    /// Its top left corner on the desktop, in units.
    pub at: Option<(i32, i32)>,
}

const USAGE: &str = "pleamar-wm output NAME [at X,Y] [vrr on|off] [mode WxH@HZ] [scale S] [transform 0|90|180|270]";

impl Wish {
    /// The words after the monitor's name: `at 1920,0 vrr on`.
    pub fn parse(words: &[&str]) -> Result<Wish, String> {
        let mut wish = Wish::default();
        let mut k = 0;
        while k < words.len() {
            let value = words.get(k + 1).copied().ok_or_else(|| format!("'{}' needs a value: {USAGE}", words[k]))?;
            match words[k] {
                "at" => wish.at = Some(crate::config::position(value).ok_or_else(|| format!("'{value}' is not a place: at X,Y"))?),
                "vrr" => {
                    wish.vrr = Some(match value {
                        "on" | "1" => true,
                        "off" | "0" => false,
                        _ => return Err(format!("'{value}': vrr on or vrr off")),
                    })
                }
                "mode" => wish.mode = Some(crate::config::mode_wish(value).ok_or_else(|| format!("'{value}' is not a mode: WxH or WxH@HZ"))?),
                "scale" => wish.scale = Some(value.parse().ok().filter(|s: &f64| *s > 0.0).ok_or_else(|| format!("'{value}' is not a scale"))?),
                "transform" => {
                    wish.transform = Some(match value {
                        "0" | "normal" => 0,
                        "90" | "1" => 1,
                        "180" | "2" => 2,
                        "270" | "3" => 3,
                        _ => return Err(format!("'{value}': transform 0, 90, 180 or 270")),
                    })
                }
                other => return Err(format!("'{other}' is not something a monitor is told: {USAGE}")),
            }
            k += 2;
        }
        if wish == Wish::default() {
            return Err(format!("nothing to change: {USAGE}"));
        }
        Ok(wish)
    }

    /// What it asks for that is not done while the session runs yet: a new
    /// mode, scale or turn needs the monitor set again with buffers of the
    /// new size, which only happens at start.
    pub fn not_yet(&self) -> Option<String> {
        let parts: Vec<&str> = [(self.mode.is_some(), "mode"), (self.scale.is_some(), "scale"), (self.transform.is_some(), "transform")].into_iter().filter_map(|(asked, name)| asked.then_some(name)).collect();
        (!parts.is_empty()).then(|| format!("{} not yet: it needs the monitor set again, which happens only at start for now (a monitor line in session.conf)", parts.join(", ")))
    }
}

/// Whether variable refresh can be turned on or off: the connector's
/// `vrr_capable` and the controller's `VRR_ENABLED`, if they have them.
/// `Ok(None)`: it already is; `Ok(Some(v))`: `VRR_ENABLED` to write.
pub fn vrr_change(name: &str, capable: Option<u64>, enabled: Option<u64>, on: bool) -> Result<Option<u64>, String> {
    let want = on as u64;
    if on && capable.is_none_or(|c| c == 0) {
        return Err(format!("{name} cannot vary its refresh: its connector does not say it can (vrr_capable)"));
    }
    match enabled {
        // Nothing to turn off where it cannot be on.
        None if !on => Ok(None),
        None => Err(format!("{name}: its controller has no VRR_ENABLED")),
        Some(now) if (now != 0) == on => Ok(None),
        Some(_) => Ok(Some(want)),
    }
}

/// Two monitors that would cover the same part of the desktop, by name, if
/// any: boxes of x, y, width and height in units.
pub fn overlapping(boxes: &[(&str, [i32; 4])]) -> Option<(String, String)> {
    for (i, (a, ra)) in boxes.iter().enumerate() {
        for (b, rb) in &boxes[i + 1..] {
            if ra[0] < rb[0] + rb[2] && rb[0] < ra[0] + ra[2] && ra[1] < rb[1] + rb[3] && rb[1] < ra[1] + ra[3] {
                return Some((a.to_string(), b.to_string()));
            }
        }
    }
    None
}

/// A wish for a monitor by its name, and where its answer goes.
pub type Asked = (String, Wish, mpsc::Sender<Result<String, String>>);
type Sink = Box<dyn Fn(Asked) -> bool + Send>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Who does it: the session, or a headless one.
pub fn set_sink(sink: Sink) {
    *SINK.lock().unwrap() = Some(sink);
}

/// Asks for it and waits for the answer.
fn apply(name: &str, wish: Wish) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    let sent = SINK.lock().unwrap().as_ref().is_some_and(|sink| sink((name.to_owned(), wish, tx)));
    if !sent {
        return Err("this session has no monitors of its own to change".to_owned());
    }
    rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| Err("the session did not answer".to_owned()))
}

/// One line, `NAME words…`, and its answer, `ok …` or `err …`.
fn answer(line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    let result = match words.split_first() {
        Some((name, rest)) => Wish::parse(rest).and_then(|w| apply(name, w)),
        None => Err(USAGE.to_owned()),
    };
    match result {
        Ok(done) => format!("ok {done}"),
        Err(why) => format!("err {why}"),
    }
}

static SERVING: OnceLock<()> = OnceLock::new();

/// Starts listening in `dir` (once): `dir/output.sock`, yours only.
pub fn serve(dir: &str) {
    let dir = dir.to_owned();
    SERVING.get_or_init(move || {
        let path = format!("{dir}/output.sock");
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("output · {path} could not be opened: {e}");
                return;
            }
        };
        let _ = std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
        let _ = std::thread::Builder::new().name("output".into()).spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_ok() {
                    let _ = writeln!(stream, "{}", answer(line.trim()));
                }
            }
        });
    });
}

/// `pleamar-wm output NAME …` from a shell: the answer printed, 0 if done.
pub fn cli(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("{USAGE}");
        return 1;
    }
    let Some(dir) = std::env::var("PLEAMAR_SOCKETS").ok().filter(|d| !d.is_empty()) else {
        eprintln!("pleamar-wm's session is not running");
        return 1;
    };
    let reply = UnixStream::connect(format!("{dir}/output.sock")).and_then(|mut s| {
        writeln!(s, "{}", args.join(" "))?;
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply)?;
        Ok(reply)
    });
    match reply {
        Ok(r) => match r.trim_end().split_once(' ') {
            Some(("ok", done)) => {
                println!("{done}");
                0
            }
            Some(("err", why)) => {
                eprintln!("{why}");
                1
            }
            _ => {
                eprintln!("output · no answer from the session");
                1
            }
        },
        Err(e) => {
            eprintln!("output · {dir}/output.sock: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wish_is_read_whole() {
        let w = Wish::parse(&["at", "1920,0", "vrr", "on"]).unwrap();
        assert_eq!(w, Wish { at: Some((1920, 0)), vrr: Some(true), ..Default::default() });
        assert_eq!(Wish::parse(&["at", "-1080x0"]).unwrap().at, Some((-1080, 0)));
        assert_eq!(Wish::parse(&["mode", "3840x2160@120", "transform", "90"]).unwrap(), Wish { mode: Some(ModeWish::Exact(3840, 2160, 120.0)), transform: Some(1), ..Default::default() });
        assert!(Wish::parse(&[]).is_err());
        assert!(Wish::parse(&["vrr"]).is_err());
        assert!(Wish::parse(&["vrr", "maybe"]).is_err());
        assert!(Wish::parse(&["at", "left"]).is_err());
        assert!(Wish::parse(&["brightness", "50"]).is_err());
    }

    /// What needs a new mode is named, and only that.
    #[test]
    fn a_new_mode_is_not_yet() {
        assert_eq!(Wish { at: Some((0, 0)), vrr: Some(false), ..Default::default() }.not_yet(), None);
        let w = Wish::parse(&["vrr", "on", "scale", "1.5", "mode", "1920x1080"]).unwrap();
        assert!(w.not_yet().unwrap().starts_with("mode, scale not yet"));
    }

    #[test]
    fn vrr_where_it_can_be() {
        assert_eq!(vrr_change("DP-3", Some(1), Some(0), true), Ok(Some(1)));
        assert_eq!(vrr_change("DP-3", Some(1), Some(1), false), Ok(Some(0)));
        assert_eq!(vrr_change("DP-3", Some(1), Some(1), true), Ok(None));
        assert!(vrr_change("HDMI-A-1", Some(0), Some(0), true).unwrap_err().contains("cannot vary"));
        assert!(vrr_change("HEADLESS-1", None, None, true).is_err());
        // Off where it cannot be on is no error: it already is.
        assert_eq!(vrr_change("HEADLESS-1", None, None, false), Ok(None));
    }

    #[test]
    fn monitors_side_by_side_do_not_overlap() {
        let side = [("DP-3", [0, 0, 1920, 1080]), ("HDMI-A-1", [1920, 0, 1280, 720])];
        assert_eq!(overlapping(&side), None);
        let over = [("DP-3", [0, 0, 1920, 1080]), ("HDMI-A-1", [1000, 500, 1280, 720])];
        assert_eq!(overlapping(&over), Some(("DP-3".to_owned(), "HDMI-A-1".to_owned())));
        let above = [("DP-3", [0, 1080, 1920, 1080]), ("HDMI-A-1", [0, 0, 1280, 1080])];
        assert_eq!(overlapping(&above), None);
    }
}
