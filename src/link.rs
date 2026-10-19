//! `pleamar-wm link …`: what an agent in a terminal does with the windows
//! wired to its own, on a canvas.
//!
//! A wire between two windows (dragged from one's port to the other) is the
//! person's word that what runs in one may read, type in and hand tasks to
//! the other. This is how: it finds the caller's window by its terminal
//! (`PLEAMAR_TERMINAL`, the layer of term.rs, and the window of that
//! terminal's process), the wires from the window manager's scene, and the
//! terminal of the window at the other end; then reads and types through
//! that terminal's layer. A window wired to the caller's that runs no
//! terminal through a layer can be listed, not read nor typed in.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

type Info = BTreeMap<String, String>;

/// A window of the session: its place in the scene and its process.
struct Win {
    slot: usize,
    pid: u32,
    title: String,
}

/// The session's windows, as pleamar-wm writes them (`windows` next to its
/// other sockets): `slot pid<TAB>title`, one a line.
fn windows() -> Result<Vec<Win>, String> {
    let dir = std::env::var("PLEAMAR_SOCKETS").ok().filter(|d| !d.is_empty()).ok_or("not inside a pleamar-wm session")?;
    let text = std::fs::read_to_string(format!("{dir}/windows")).map_err(|_| "this pleamar-wm session says nothing of its windows (an older pleamar-wm?)".to_string())?;
    Ok(text
        .lines()
        .filter_map(|l| {
            let (head, title) = l.split_once('\t').unwrap_or((l, ""));
            let mut w = head.split_whitespace();
            Some(Win { slot: w.next()?.parse().ok()?, pid: w.next()?.parse().ok()?, title: title.to_owned() })
        })
        .collect())
}

/// Orders to the window manager's scene, answered in order.
fn say(orders: &[String]) -> Vec<String> {
    let me = std::env::current_exe().unwrap_or_else(|_| "pleamar-wm".into());
    let out = std::process::Command::new(me).arg("--say").arg("wm").args(orders).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_owned).collect()).unwrap_or_default()
}

/// The wires: which slots each joins, by its place (`wire.K`), and what the
/// person called it.
fn wires() -> Vec<(usize, i64, i64, String)> {
    let orders: Vec<String> = (0..8).flat_map(|k| [format!("get wire.{k}.a"), format!("get wire.{k}.b")]).collect();
    let got: Vec<i64> = say(&orders).iter().map(|v| v.trim().parse::<f64>().map_or(-1, |v| v.round() as i64)).collect();
    (0..8)
        .filter_map(|k| Some((k, *got.get(2 * k)?, *got.get(2 * k + 1)?)))
        .filter(|(_, a, b)| *a >= 0 && *b >= 0)
        .map(|(k, a, b)| (k, a, b, say(&[format!("get wire.{k}.label")]).first().map(|l| l.trim().to_owned()).unwrap_or_default()))
        .collect()
}

/// The terminal layer whose terminal is that process, if one.
fn layer_of(pid: u32, layers: &[(i32, Info)]) -> Option<(i32, Info)> {
    let mut of: Vec<&(i32, Info)> = layers.iter().filter(|(_, i)| i.get("terminal").and_then(|t| t.parse::<u32>().ok()) == Some(pid)).collect();
    // A terminal with several shells (tabs) speaks for the one that moved last.
    of.sort_by_key(|(_, i)| i.get("quiet").and_then(|q| q.parse::<u64>().ok()).unwrap_or(u64::MAX));
    of.first().map(|l| (*l).clone())
}

struct Here {
    layer: i32,
    info: Info,
    slot: usize,
    wins: Vec<Win>,
    layers: Vec<(i32, Info)>,
    wires: Vec<(usize, i64, i64, String)>,
}

/// Who calls: its terminal's layer, and the window that terminal is.
fn here() -> Result<Here, String> {
    let layer: i32 = std::env::var("PLEAMAR_TERMINAL").ok().and_then(|v| v.parse().ok()).ok_or("this is not a terminal of a pleamar-wm session (no PLEAMAR_TERMINAL): only an agent in one can use the windows wired to it")?;
    let layers = crate::term::layers();
    let info = layers.iter().find(|(id, _)| *id == layer).map(|(_, i)| i.clone()).ok_or("this terminal's layer does not answer")?;
    let terminal: u32 = info.get("terminal").and_then(|t| t.parse().ok()).unwrap_or(0);
    let wins = windows()?;
    let slot = wins.iter().find(|w| w.pid == terminal).map(|w| w.slot).ok_or("this terminal's window is not one of the session's")?;
    Ok(Here { layer, info, slot, wins, layers, wires: wires() })
}

impl Here {
    /// The windows wired to the caller's, with the wire's place and which end the caller is.
    fn wired(&self) -> Vec<(usize, &Win, bool)> {
        self.wires
            .iter()
            .filter_map(|(k, a, b, _)| {
                let (mine_is_a, other) = if *a as usize == self.slot { (true, *b) } else if *b as usize == self.slot { (false, *a) } else { return None };
                self.wins.iter().find(|w| w.slot == other as usize).map(|w| (*k, w, mine_is_a))
            })
            .collect()
    }

    /// What the person called that wire, if anything.
    fn label(&self, k: usize) -> &str {
        self.wires.iter().find(|w| w.0 == k).map_or("", |w| w.3.as_str())
    }

    /// The one named: by its number, or by what runs in it if only one is that.
    fn target(&self, name: &str) -> Result<(usize, &Win, bool, Option<(i32, Info)>), String> {
        let wired = self.wired();
        let found: Vec<&(usize, &Win, bool)> = match name.trim_start_matches('#').parse::<usize>() {
            Ok(n) => wired.iter().filter(|(_, w, _)| w.slot == n).collect(),
            // By the wire's name first («tests»), then by what runs there.
            Err(_) if wired.iter().any(|(k, _, _)| self.label(*k).eq_ignore_ascii_case(name)) => wired.iter().filter(|(k, _, _)| self.label(*k).eq_ignore_ascii_case(name)).collect(),
            Err(_) => wired
                .iter()
                .filter(|(_, w, _)| {
                    let l = layer_of(w.pid, &self.layers);
                    l.as_ref().and_then(|(_, i)| i.get("agent")).is_some_and(|a| a == name) || w.title.to_lowercase().contains(&name.to_lowercase())
                })
                .collect(),
        };
        match found.as_slice() {
            [(k, w, a)] => Ok((*k, w, *a, layer_of(w.pid, &self.layers))),
            [] if name.parse::<usize>().is_ok() => Err(format!("window {name} isn't wired to yours. Ask the person to wire them: on a canvas (Super+Shift+W), drag the drop at the start of one's title bar to the other.")),
            [] => Err(format!("no window wired to yours is «{name}». `pleamar-wm link list` says which are.")),
            _ => Err(format!("more than one window wired to yours is «{name}»: name it by its number (`pleamar-wm link list`).")),
        }
    }
}

fn line_of(slot: usize, w: &Win, layer: Option<&(i32, Info)>, label: &str) -> String {
    let g = |k: &str| layer.and_then(|(_, i)| i.get(k)).cloned().unwrap_or_default();
    let what = match layer {
        None => "no terminal of its own to read or type in".to_owned(),
        Some(_) => {
            let agent = g("agent");
            let state = match (agent.is_empty(), g("state").as_str()) {
                (true, _) => String::new(),
                (false, "working") => " (working)".into(),
                (false, "idle") => " (ready)".into(),
                _ => String::new(),
            };
            let runs = if agent.is_empty() { g("command") } else { agent };
            format!("{runs}{state} in {}", tilde(&g("cwd")))
        }
    };
    let named = if label.is_empty() { String::new() } else { format!("[{label}]  ") };
    format!("{slot}  {named}«{}»  {what}", w.title)
}

fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && path.starts_with(&h) => format!("~{}", &path[h.len()..]),
        _ => path.to_owned(),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn socket(id: i32) -> String {
    format!("{}/{id}.sock", crate::term::socket_dir())
}

fn info(id: i32) -> Info {
    crate::term::ask(&socket(id), "info")
        .map(|t| t.lines().filter_map(|l| l.split_once(' ').map(|(k, v)| (k.to_owned(), v.to_owned())).or_else(|| Some((l.to_owned(), String::new())))).collect())
        .unwrap_or_default()
}

fn read(id: i32, lines: usize) -> String {
    crate::term::ask(&socket(id), &format!("read {lines}")).unwrap_or_default()
}

fn paste(id: i32, text: &str) -> Result<(), String> {
    match crate::term::ask(&socket(id), &format!("paste {}", hex(text.as_bytes())))? {
        r if r.starts_with("ok") => Ok(()),
        r => Err(r.trim().to_owned()),
    }
}

fn keys(id: i32, names: &str) -> Result<(), String> {
    match crate::term::ask(&socket(id), &format!("keys {names}"))? {
        r if r.starts_with("ok") => Ok(()),
        r => Err(r.trim().to_owned()),
    }
}

/// Done with what it was doing: its agent shows it is ready (✳), or —no
/// agent's title to go by— nothing has come out of it for a while.
fn at_rest(i: &Info) -> bool {
    match i.get("state").map(String::as_str) {
        Some("idle") => true,
        Some("working") => false,
        _ => i.get("quiet").and_then(|q| q.parse::<u64>().ok()).is_some_and(|q| q > 4000),
    }
}

/// How a wait ended.
enum Waited {
    /// At rest, asking nothing.
    Done,
    /// It stopped to ask its person something (a permission, a choice).
    Asks(crate::term::Question),
    /// Its terminal is gone.
    Gone,
    /// Still at it when the time was up.
    Late,
}

/// The question on that terminal's screen, if its agent is not working.
fn asked(id: i32, i: &Info) -> Option<crate::term::Question> {
    (i.get("state").map(String::as_str) != Some("working")).then(|| crate::term::question(&read(id, 40))).flatten()
}

/// Waits for that terminal to be done —at rest twice in a row, asking
/// nothing: a title that blinks between frames is not an end—, or, with
/// `asks`, to stop on a question (seen twice too).
fn wait_for(id: i32, timeout: Duration, asks: bool) -> Waited {
    let start = Instant::now();
    let (mut rest, mut asking) = (0, 0);
    while start.elapsed() < timeout {
        let i = info(id);
        if i.is_empty() {
            return Waited::Gone;
        }
        match asked(id, &i) {
            Some(q) => {
                rest = 0;
                asking += 1;
                if asks && asking >= 2 {
                    return Waited::Asks(q);
                }
            }
            None => {
                asking = 0;
                rest = if at_rest(&i) { rest + 1 } else { 0 };
                if rest >= 2 {
                    return Waited::Done;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Waited::Late
}

/// Waits for that terminal to be at rest, up to a time. Whether it got there.
fn wait_rest(id: i32, timeout: Duration) -> bool {
    matches!(wait_for(id, timeout, false), Waited::Done)
}

/// A question, said so an agent can answer it.
fn said(slot: usize, q: &crate::term::Question) -> String {
    let mut out = format!("window {slot} asks: {}\n", q.text);
    if q.yes_no {
        out.push_str(&format!("Answer: pleamar-wm link answer {slot} yes|no"));
    } else {
        for (k, (n, t)) in q.options.iter().enumerate() {
            out.push_str(&format!("  {n}) {t}{}\n", if k == q.cursor { "   ← selected now" } else { "" }));
        }
        out.push_str(&format!("Answer: pleamar-wm link answer {slot} NUMBER (or the words of an option; free words go where it lets you type)"));
    }
    out
}

/// What has come out since the task was handed: after the line that
/// says what it is (the last one, if it handed several), or since a reading
/// `before` had that many lines.
fn since(id: i32, before: usize) -> String {
    let all = read(id, 2000);
    let lines: Vec<&str> = all.lines().collect();
    let from = match lines.iter().rposition(|l| l.contains("through a wire]")) {
        Some(k) => k + 1,
        None => before.saturating_sub(3).min(lines.len()),
    };
    answer(&lines[from..])
}

/// Of what came out after the task, the answer: from the agent's first
/// message (Claude Code's `●`/`⏺`, Codex's `•`) if it marks them, and
/// without what an agent draws under it —its input box between two rules,
/// the prompt in it, the line under it, «done in 7s»—. At most 80 lines.
fn answer(lines: &[&str]) -> String {
    let rule = |l: &str| {
        let t = l.trim();
        t.chars().count() >= 20 && t.chars().all(|c| matches!(c, '─' | '━' | '-' | '═' | '╌'))
    };
    // Its input box: the first rule after which, before another rule, its prompt is.
    let prompt = |l: &str| matches!(l.trim_start().chars().next(), Some('❯' | '›' | '>'));
    let mut end = lines.len();
    for (k, l) in lines.iter().enumerate() {
        if rule(l) && lines[k + 1..].iter().take_while(|n| !rule(n)).any(|n| prompt(n)) {
            end = k;
            break;
        }
    }
    let body = &lines[..end];
    let start = body.iter().position(|l| matches!(l.trim_start().chars().next(), Some('●' | '⏺' | '•'))).unwrap_or(0);
    let mut kept: Vec<&str> = body[start..].iter().map(|l| l.trim_end()).collect();
    // Below the answer: blank lines, and how long it thought (`✻ Cogitated for 7s · done 20:25`).
    while kept.last().is_some_and(|l| l.is_empty() || matches!(l.trim_start().chars().next(), Some('✻' | '✳' | '✶' | '✽' | '✢' | '·'))) {
        kept.pop();
    }
    let keep = kept.len().saturating_sub(80);
    kept[keep..].join("\n")
}

fn flow(k: usize, v: i32) {
    say(&[format!("fact wire.{k}.flow {v}")]);
}

fn number(args: &[String], flag: &str, default: u64) -> u64 {
    args.iter().position(|a| a == flag).and_then(|p| args.get(p + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn words(args: &[String]) -> String {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        match a.as_str() {
            "--timeout" | "--lines" => skip = true,
            "--enter" | "--wait" => {}
            _ => out.push(a.as_str()),
        }
    }
    out.join(" ")
}

pub fn run(args: &[String]) -> i32 {
    match go(args) {
        Ok(out) => {
            if !out.is_empty() {
                println!("{}", out.trim_end());
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

const USAGE: &str = "pleamar-wm link — the windows wired to yours (on a canvas, the person wires them)
  whoami                          which window you are, and what is wired to it
  label N NAME                    names the wire to N («tests»): shown on it, and N can be NAME
  list                            the windows wired to yours: number, title, what runs there
  read N [LINES]                  the last lines of its terminal (200)
  send N TEXT [--enter]           TEXT typed into it as a paste; --enter presses Enter after
  keys N KEY…                     keys: enter tab escape up down left right backspace ctrl+c y 1 …
  wait N [--timeout S]            until its agent is done (ready again), then its answer; or what
                                  it stopped to ask
  ask N                           what it asks its person now, if anything: the question and options
  answer N NUMBER|WORDS|yes|no    answers that: an option by number or words, words where it lets
                                  you type them (the person sees on it who answered)
  delegate N TASK [--wait] [--timeout S]
                                  hands TASK to the agent there. Without --wait it returns at
                                  once, and its answer —or a question it stops to ask on the
                                  way— is typed into your terminal; with --wait, it is what
                                  this prints.
N is a window's number from `list`, its wire's name (`tests`), or the agent that runs there
(`codex`) if only one is.";

fn go(args: &[String]) -> Result<String, String> {
    let verb = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or(&[]);
    match verb {
        "whoami" => {
            let h = here()?;
            let me = h.wins.iter().find(|w| w.slot == h.slot).map(|w| w.title.clone()).unwrap_or_default();
            let agent = h.info.get("agent").filter(|a| !a.is_empty()).cloned().unwrap_or_else(|| "a shell".into());
            let mut out = format!("You are window {} «{me}»: {agent} in {} (terminal {}).\n", h.slot, tilde(h.info.get("cwd").map_or("", String::as_str)), h.layer);
            let wired = h.wired();
            if wired.is_empty() {
                out.push_str("No window is wired to yours.");
            } else {
                out.push_str("Wired to it:\n");
                for (k, w, _) in wired {
                    out.push_str(&format!("  {}\n", line_of(w.slot, w, layer_of(w.pid, &h.layers).as_ref(), h.label(k))));
                }
            }
            Ok(out)
        }
        "list" => {
            let h = here()?;
            let wired = h.wired();
            if wired.is_empty() {
                return Ok("No window is wired to yours. The person wires them: on a canvas (Super+Shift+W), dragging the drop at the start of a window's title bar to another window.".into());
            }
            Ok(wired.iter().map(|(k, w, _)| line_of(w.slot, w, layer_of(w.pid, &h.layers).as_ref(), h.label(*k))).collect::<Vec<_>>().join("\n"))
        }
        // The wire to that window, named («tests»): shown on it, and a name to give it.
        "label" => {
            let name = rest.first().ok_or(USAGE)?;
            let h = here()?;
            let (k, w, _, _) = h.target(name)?;
            let text = words(&rest[1..]).replace(['\n', '\t'], " ");
            say(&[format!("text wire.{k}.label {text}")]);
            Ok(if text.is_empty() { format!("the wire to window {} has no name now", w.slot) } else { format!("the wire to window {} is «{text}»", w.slot) })
        }
        "read" | "send" | "keys" | "wait" | "ask" | "answer" | "delegate" => {
            let name = rest.first().ok_or(USAGE)?;
            let h = here()?;
            let (k, w, mine_is_a, layer) = h.target(name)?;
            let (id, tinfo) = layer.ok_or_else(|| format!("window {} «{}» has no terminal of its own (through pleamar-wm's layer) to read or type in", w.slot, w.title))?;
            match verb {
                "read" => Ok(read(id, rest.get(1).and_then(|n| n.parse().ok()).unwrap_or(200))),
                "send" => {
                    let text = words(&rest[1..]);
                    if text.is_empty() {
                        return Err("send N TEXT [--enter]".into());
                    }
                    paste(id, &text)?;
                    if rest.iter().any(|a| a == "--enter") {
                        std::thread::sleep(Duration::from_millis(150));
                        keys(id, "enter")?;
                    }
                    Ok(format!("typed into window {}", w.slot))
                }
                "keys" => {
                    keys(id, &rest[1..].join(" "))?;
                    std::thread::sleep(Duration::from_millis(300));
                    Ok(read(id, 30))
                }
                "wait" => {
                    let timeout = Duration::from_secs(number(rest, "--timeout", 600));
                    match wait_for(id, timeout, true) {
                        Waited::Done => Ok(answer(&read(id, 120).lines().collect::<Vec<_>>())),
                        Waited::Asks(q) => Ok(said(w.slot, &q)),
                        Waited::Gone => Err(format!("window {}'s terminal is gone", w.slot)),
                        Waited::Late => Err(format!("window {} is still at it after {} s", w.slot, timeout.as_secs())),
                    }
                }
                "ask" => {
                    let i = info(id);
                    Ok(match asked(id, &i) {
                        Some(q) => said(w.slot, &q),
                        None => format!("window {} asks nothing now", w.slot),
                    })
                }
                "answer" => {
                    let reply = words(&rest[1..]);
                    if reply.is_empty() {
                        return Err("answer N NUMBER|yes|no|WORDS".into());
                    }
                    let i = info(id);
                    let q = asked(id, &i).ok_or_else(|| format!("window {} asks nothing now (`send` types in it)", w.slot))?;
                    let chose = choose(id, &q, &reply).map_err(|e| format!("{e}\n{}", said(w.slot, &q)))?;
                    // Said on it a while: whose answer this was.
                    say(&[format!("emit answered {}", w.slot * 100 + h.slot + 1)]);
                    std::thread::sleep(Duration::from_millis(700));
                    Ok(format!("answered window {}: {chose}", w.slot))
                }
                _ => {
                    let task = words(&rest[1..]);
                    if task.is_empty() {
                        return Err("delegate N TASK [--wait] [--timeout S]".into());
                    }
                    if tinfo.get("agent").is_none_or(|a| a.is_empty()) {
                        return Err(format!("window {} runs no agent ({}): `send` types in it instead", w.slot, tinfo.get("command").cloned().unwrap_or_default()));
                    }
                    if tinfo.get("state").map(String::as_str) == Some("working") {
                        return Err(format!("window {}'s agent is busy: `wait {}` until it is done", w.slot, w.slot));
                    }
                    let me = h.wins.iter().find(|w| w.slot == h.slot).map(|w| w.title.clone()).unwrap_or_default();
                    let who = h.info.get("agent").filter(|a| !a.is_empty()).cloned().unwrap_or_else(|| "a shell".into());
                    let before = read(id, 2000).lines().count();
                    // The drop runs along the wire, towards it, while it works.
                    flow(k, if mine_is_a { 1 } else { -1 });
                    // A line that says what this is, as a paste of its own (a long
                    // paste is taken as data, not as something asked), the task
                    // after it, and Enter.
                    paste(id, &format!("[a task from {who} in window {} «{me}», through a wire] It is in the pasted text: do it, and end with your answer.", h.slot))?;
                    std::thread::sleep(Duration::from_millis(120));
                    paste(id, &task)?;
                    std::thread::sleep(Duration::from_millis(150));
                    keys(id, "enter")?;
                    // Taken? Its agent starts working (or something comes out).
                    let started = (0..10).any(|_| {
                        std::thread::sleep(Duration::from_millis(400));
                        let i = info(id);
                        i.get("state").map(String::as_str) == Some("working") || i.get("quiet").and_then(|q| q.parse::<u64>().ok()).is_some_and(|q| q < 300)
                    });
                    if !started {
                        keys(id, "enter")?;
                    }
                    let timeout = number(rest, "--timeout", 600);
                    // A shell has nowhere to be told the answer later (it would
                    // run it): it waits for it.
                    let caller_agent = h.info.get("agent").is_some_and(|a| !a.is_empty());
                    if rest.iter().any(|a| a == "--wait") || !caller_agent {
                        return match wait_for(id, Duration::from_secs(timeout), true) {
                            Waited::Done => {
                                back(k, mine_is_a);
                                Ok(since(id, before))
                            }
                            Waited::Asks(q) => {
                                flow(k, 0);
                                Ok(format!("{}\nIt stopped to ask that while on your task. Once answered, `pleamar-wm link wait {}` brings its answer.", said(w.slot, &q), w.slot))
                            }
                            Waited::Gone => Err(format!("window {}'s terminal went away", w.slot)),
                            Waited::Late => {
                                flow(k, 0);
                                Err(format!("window {} did not finish in {timeout} s; so far:\n{}", w.slot, since(id, before)))
                            }
                        };
                    }
                    // Not waited for: a process of its own waits, and types the
                    // answer into this terminal when it is there.
                    let me_exe = std::env::current_exe().map_err(|e| e.to_string())?;
                    let mut c = std::process::Command::new("setsid");
                    c.arg("-f").arg(me_exe).args(["link", "_deliver", &id.to_string(), &h.layer.to_string(), &k.to_string(), &(mine_is_a as u8).to_string(), &before.to_string(), &timeout.to_string(), &w.slot.to_string()]);
                    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
                    c.spawn().map_err(|e| e.to_string())?;
                    Ok(format!("Handed to window {} «{}». Its answer will be typed into this terminal when it is done; or `pleamar-wm link wait {}`.", w.slot, w.title, w.slot))
                }
            }
        }
        // (The process `delegate` leaves to bring the answer back.)
        "_deliver" => {
            let n = |i: usize| rest.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let (id, mine, k, mine_is_a, before, timeout, slot) = (n(0) as i32, n(1) as i32, n(2) as usize, n(3) == 1, n(4) as usize, n(5) as u64, n(6));
            let deadline = Instant::now() + Duration::from_secs(timeout);
            let mut told = String::new();
            let done = loop {
                match wait_for(id, deadline.saturating_duration_since(Instant::now()), true) {
                    Waited::Done => break true,
                    Waited::Gone | Waited::Late => break false,
                    // It stopped to ask its person: the one who handed it the
                    // task is told (once for each question), and may answer.
                    Waited::Asks(q) => {
                        if q.text != told || told.is_empty() {
                            told = q.text.clone();
                            tell(mine, &format!("[window {slot} stopped to ask something while on the task you handed it] It is in the pasted text."), &said(slot as usize, &q))?;
                        }
                        // Until it is answered (by anyone), or the time is up.
                        while Instant::now() < deadline && asked(id, &info(id)).is_some_and(|n| n.text == told) {
                            std::thread::sleep(Duration::from_millis(700));
                        }
                    }
                }
            };
            back(k, mine_is_a);
            let answer = since(id, before);
            let head = if done { format!("[the answer from window {slot}, to the task you handed it] It is in the pasted text.") } else { format!("[window {slot} did not finish your task in {timeout} s] What it said so far is in the pasted text.") };
            tell(mine, &head, &answer)?;
            Ok(String::new())
        }
        _ => Err(USAGE.into()),
    }
}

/// Into the asking agent's terminal, once it is free —not over its person
/// typing, nor into the middle of its own work—: a line, the text, Enter.
fn tell(mine: i32, head: &str, text: &str) -> Result<(), String> {
    wait_rest(mine, Duration::from_secs(3600));
    paste(mine, head)?;
    std::thread::sleep(Duration::from_millis(120));
    paste(mine, text)?;
    std::thread::sleep(Duration::from_millis(150));
    keys(mine, "enter")
}

/// Answers a question on that terminal: an option by its number or its
/// words (the cursor taken there, and Enter), words where it lets you type
/// them, or yes/no. What was chosen.
fn choose(id: i32, q: &crate::term::Question, reply: &str) -> Result<String, String> {
    let r = reply.trim();
    if q.yes_no {
        let yes = matches!(r.to_lowercase().as_str(), "y" | "yes" | "s" | "si" | "sí" | "1" | "true" | "ok");
        keys(id, if yes { "y" } else { "n" })?;
        std::thread::sleep(Duration::from_millis(60));
        keys(id, "enter")?;
        return Ok(if yes { "yes".into() } else { "no".into() });
    }
    let low = r.to_lowercase();
    let by_number = r.parse::<u32>().ok().and_then(|n| q.options.iter().position(|o| o.0 == n));
    let by_words = q.options.iter().position(|o| o.1.to_lowercase().contains(&low));
    // Free words: the option that lets you type (Claude Code's «Type something»,
    // «No, and tell Claude what to do differently»).
    let typing = q.options.iter().position(|o| {
        let t = o.1.to_lowercase();
        t.contains("type something") || t.contains("tell claude") || t.starts_with("other") || t.contains("something else")
    });
    let (k, then_type) = match (by_number, by_words, typing) {
        (Some(k), _, _) => (k, false),
        (None, Some(k), _) => (k, false),
        (None, None, Some(k)) => (k, true),
        _ => return Err(format!("«{r}» is none of its options")),
    };
    let steps = k as i64 - q.cursor as i64;
    let mut ks: Vec<&str> = vec![if steps > 0 { "down" } else { "up" }; steps.unsigned_abs() as usize];
    ks.push("enter");
    keys(id, &ks.join(" "))?;
    if then_type {
        std::thread::sleep(Duration::from_millis(350));
        paste(id, r)?;
        std::thread::sleep(Duration::from_millis(150));
        keys(id, "enter")?;
        return Ok(format!("«{}», then wrote: {r}", q.options[k].1));
    }
    Ok(format!("{}) {}", q.options[k].0, q.options[k].1))
}

/// The answer goes back along the wire, a moment, and it is still again.
fn back(k: usize, mine_is_a: bool) {
    flow(k, if mine_is_a { -1 } else { 1 });
    std::thread::sleep(Duration::from_millis(1600));
    flow(k, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claude_code_answer_comes_without_its_input_box() {
        let screen = "  (trabajando en ~/Proyectos/pleamar). Esto es una prueba de los cables de pleamar-wm:
  respóndeme en una sola frase quién eres y en qué carpeta estás. No hagas nada más.

● Soy Claude Code (Opus 5.5) y estoy trabajando en la carpeta /home/abel.

✻ Cogitated for 7s · done 20:25



───────────────────────────────────────────────────────────────────────────────────────────
❯
───────────────────────────────────────────────────────────────────────────────────────────
  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← 1 agent";
        let lines: Vec<&str> = screen.lines().collect();
        assert_eq!(answer(&lines), "● Soy Claude Code (Opus 5.5) y estoy trabajando en la carpeta /home/abel.");
    }

    #[test]
    fn an_answer_of_several_lines_keeps_them_all() {
        let lines = ["task tail", "", "● First line.", "  second line", "", "● And a last word.", "", "────────────────────────", "❯ ", "────────────────────────"];
        assert_eq!(answer(&lines), "● First line.\n  second line\n\n● And a last word.");
    }

    #[test]
    fn an_agent_that_marks_nothing_gives_what_it_said() {
        let lines = ["ANSWER: FIND THE LEAK", ">"];
        assert_eq!(answer(&lines), "ANSWER: FIND THE LEAK\n>");
    }
}
