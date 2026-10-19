//! The layer between a terminal and its shell: what lets a window manager
//! read and write in any terminal, whichever it is.
//!
//! In a pleamar-wm session `SHELL` names a link to this program called as the
//! user's shell (`~/.local/share/pleamar-wm/shell/fish`), and the real one is
//! in `PLEAMAR_REAL_SHELL`. A terminal that starts `$SHELL` —kitty, Alacritty,
//! foot, Konsole, WezTerm…— starts this instead, which starts the real shell
//! on a terminal of its own (a pty) and passes everything through, both ways,
//! untouched: the terminal draws what the shell says, as ever. On the way it
//! keeps what is on the screen, as a terminal would (`vt100`), and listens on
//! a socket of its own (`$XDG_RUNTIME_DIR/pleamar/term/PID.sock`): what is
//! there, its title, what runs in it, and text or keys to type. What runs in
//! it knows it from `PLEAMAR_TERMINAL` (this process's id).
//!
//! It never stands in the way: asked to run a command (`-c`), not on a
//! terminal, inside another one of these, or failing to set itself up, it is
//! the real shell at once.

use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::sync::{Arc, Mutex};

/// History kept beyond the screen, in lines.
const HISTORY: usize = 2000;

/// The folder of the links, next to Build's: `~/.local/share/pleamar-wm/shell`.
fn link_dir() -> String {
    format!("{}/shell", crate::build::dir().trim_end_matches("/build"))
}

/// Where the layers listen.
pub fn socket_dir() -> String {
    let run = std::env::var("XDG_RUNTIME_DIR").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| "/tmp".into());
    format!("{run}/pleamar/term")
}

/// The user's shell, as the system says it: `SHELL`, or the passwd entry.
fn users_shell() -> Option<String> {
    if let Some(s) = std::env::var("SHELL").ok().filter(|s| !s.is_empty() && !s.contains("/pleamar-wm/shell/")) {
        return Some(s);
    }
    // SAFETY: getpwuid's answer is read at once, before anything else asks.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_shell.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*pw).pw_shell).to_string_lossy().into_owned()).filter(|s| !s.is_empty())
    }
}

static SESSION: std::sync::OnceLock<Option<(String, String)>> = std::sync::OnceLock::new();

/// The session's: made once (`shell_link`), unless `terminals off`.
pub fn session_shell() -> Option<&'static (String, String)> {
    SESSION.get_or_init(|| if crate::config::get().terminals_off { None } else { shell_link() }).as_ref()
}

/// For the session: the link that stands for the user's shell, and the real
/// one, to give every program it starts (`SHELL`, `PLEAMAR_REAL_SHELL`).
/// Named as the shell is, so what looks at `$SHELL`'s name still sees fish.
fn shell_link() -> Option<(String, String)> {
    let real = users_shell()?;
    let name = std::path::Path::new(&real).file_name()?.to_str()?.to_owned();
    let dir = link_dir();
    let link = format!("{dir}/{name}");
    let me = std::env::current_exe().ok()?;
    if std::fs::read_link(&link).ok().as_deref() != Some(me.as_path()) {
        std::fs::create_dir_all(&dir).ok()?;
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&me, &link).ok()?;
    }
    Some((link, real))
}

/// Called as the shell (`argv[0]` is the real shell's name, `-fish` for a
/// login one): the layer, or the shell itself. Never returns then.
pub fn as_shell() {
    let Some(real) = std::env::var("PLEAMAR_REAL_SHELL").ok().filter(|s| !s.is_empty()) else { return };
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let Some(zero) = args.first().and_then(|a| std::path::Path::new(a).file_name()).and_then(|n| n.to_str()) else { return };
    let real_name = std::path::Path::new(&real).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if zero.trim_start_matches('-') != real_name || real_name.is_empty() {
        return;
    }
    // SAFETY: isatty only asks.
    let on_terminal = unsafe { libc::isatty(0) == 1 && libc::isatty(1) == 1 };
    let a_command = args.iter().skip(1).any(|a| a == "-c" || a == "--command");
    let inside = std::env::var_os("PLEAMAR_TERMINAL").is_some();
    if on_terminal && !a_command && !inside && std::env::var_os("PLEAMAR_NO_TERM").is_none() {
        if let Err(e) = layer(&real, &args) {
            eprintln!("pleamar-wm · the terminal layer could not start ({e}): the shell as it is");
        }
    }
    exec(&real, &args, &[]);
}

/// The real shell in this process's place: its own name in front (`-fish`
/// stays a login shell), the rest of the words as they came.
fn exec(real: &str, args: &[std::ffi::OsString], env: &[(&str, String)]) -> ! {
    use std::os::unix::ffi::OsStrExt;
    let zero = args.first().map(|a| a.as_bytes().starts_with(b"-")).unwrap_or(false);
    let name = std::path::Path::new(real).file_name().map(|n| n.as_bytes().to_vec()).unwrap_or_default();
    let mut argv: Vec<std::ffi::CString> = vec![std::ffi::CString::new(if zero { [b"-".as_slice(), &name].concat() } else { name }).unwrap_or_default()];
    argv.extend(args.iter().skip(1).filter_map(|a| std::ffi::CString::new(a.as_bytes()).ok()));
    let path = std::ffi::CString::new(real).unwrap_or_default();
    // SAFETY: before exec, this process only sets its own environment.
    unsafe {
        std::env::set_var("SHELL", real);
        for (k, v) in env {
            std::env::set_var(k, v);
        }
    }
    let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    // SAFETY: argv is null-terminated and outlives the call.
    unsafe { libc::execv(path.as_ptr(), ptrs.as_ptr()) };
    eprintln!("pleamar-wm · {real}: {}", std::io::Error::last_os_error());
    std::process::exit(127);
}

/// What it keeps of the screen: the terminal's contents, and what the
/// program said of itself (its title, the bell).
struct Seen {
    title: String,
    bells: u64,
}

impl vt100::Callbacks for Seen {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }
}

struct Shared {
    screen: Mutex<vt100::Parser<Seen>>,
    // Writes to the shell's terminal, from the keyboard and from outside,
    // one at a time: a paste is not cut by a key in its middle.
    input: Mutex<()>,
    master: RawFd,
    shell: libc::pid_t,
    // When something last came out, in ms since the start.
    last_out: std::sync::atomic::AtomicU64,
    started: std::time::Instant,
}

fn size_of(fd: RawFd) -> libc::winsize {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: ws is a winsize the kernel fills.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } != 0 || ws.ws_row == 0 || ws.ws_col == 0 {
        ws.ws_row = 24;
        ws.ws_col = 80;
    }
    ws
}

fn nz(v: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(v.max(1)).unwrap_or(std::num::NonZeroU16::MIN)
}

/// The layer: the shell on a pty of its own, everything passed through.
/// Returns only if it could not set itself up (then the shell runs as it is).
fn layer(real: &str, args: &[std::ffi::OsString]) -> Result<(), String> {
    // SAFETY: plain calls on this process's own terminal and a new pty; the
    // child only sets up its terminal and execs.
    unsafe {
        let mut outer: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut outer) != 0 {
            return Err("not a terminal".into());
        }
        let ws = size_of(0);
        let (mut master, mut slave) = (-1, -1);
        if libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), &outer, &ws) != 0 {
            return Err(format!("openpty: {}", std::io::Error::last_os_error()));
        }
        // The signals that matter are waited for by a thread of their own:
        // blocked here, before any thread is made, so they all inherit it.
        let mut wanted: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut wanted);
        for s in [libc::SIGWINCH, libc::SIGHUP, libc::SIGTERM, libc::SIGCHLD] {
            libc::sigaddset(&mut wanted, s);
        }
        let mut was: libc::sigset_t = std::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, &wanted, &mut was);
        let me = libc::getpid();
        let child = libc::fork();
        if child < 0 {
            libc::pthread_sigmask(libc::SIG_SETMASK, &was, std::ptr::null_mut());
            libc::close(master);
            libc::close(slave);
            return Err(format!("fork: {}", std::io::Error::last_os_error()));
        }
        if child == 0 {
            libc::pthread_sigmask(libc::SIG_SETMASK, &was, std::ptr::null_mut());
            libc::close(master);
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
            libc::dup2(slave, 0);
            libc::dup2(slave, 1);
            libc::dup2(slave, 2);
            if slave > 2 {
                libc::close(slave);
            }
            exec(real, args, &[("PLEAMAR_TERMINAL", me.to_string())]);
        }
        libc::close(slave);
        // This terminal raw: every byte, Ctrl+C included, goes to the shell's,
        // whose own rules make of it what they make.
        let mut raw = outer;
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
        let shared = Arc::new(Shared {
            screen: Mutex::new(vt100::Parser::new_with_callbacks(nz(ws.ws_row), nz(ws.ws_col), HISTORY, Seen { title: String::new(), bells: 0 })),
            input: Mutex::new(()),
            master,
            shell: child,
            last_out: std::sync::atomic::AtomicU64::new(0),
            started: std::time::Instant::now(),
        });
        let socket = format!("{}/{me}.sock", socket_dir());
        // Typed: to the shell.
        {
            let s = shared.clone();
            std::thread::spawn(move || {
                let mut keyboard = std::fs::File::from_raw_fd(0);
                let mut buf = [0u8; 4096];
                loop {
                    match keyboard.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let _g = s.input.lock();
                            write_all(s.master, &buf[..n]);
                        }
                    }
                }
                std::mem::forget(keyboard);
            });
        }
        // The size, followed; and leaving with the shell or the terminal.
        {
            let s = shared.clone();
            let socket = socket.clone();
            std::thread::spawn(move || loop {
                let mut sig: libc::c_int = 0;
                if libc::sigwait(&wanted, &mut sig) != 0 {
                    continue;
                }
                match sig {
                    libc::SIGWINCH => {
                        let ws = size_of(0);
                        libc::ioctl(s.master, libc::TIOCSWINSZ, &ws);
                        if let Ok(mut p) = s.screen.lock() {
                            p.screen_mut().set_size(nz(ws.ws_row), nz(ws.ws_col));
                        }
                    }
                    libc::SIGHUP | libc::SIGTERM => {
                        let _ = std::fs::remove_file(&socket);
                        libc::kill(s.shell, libc::SIGHUP);
                        libc::_exit(129);
                    }
                    _ => {}
                }
            });
        }
        // Asked from outside.
        if let Some(listener) = listen(&socket) {
            let s = shared.clone();
            std::thread::spawn(move || {
                for c in listener.incoming().flatten() {
                    let s = s.clone();
                    std::thread::spawn(move || answer(&s, c));
                }
            });
        }
        // What the shell says: to the terminal at once, and kept by another
        // thread (reading it as a terminal would is slower than passing it).
        // Up to 16 MB behind; past that, the shell waits for it.
        let (keep, kept) = std::sync::mpsc::channel::<Vec<u8>>();
        let behind = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        {
            let s = shared.clone();
            let behind = behind.clone();
            std::thread::spawn(move || {
                while let Ok(mut bytes) = kept.recv() {
                    // All that has come meanwhile, at once.
                    while let Ok(more) = kept.try_recv() {
                        bytes.extend_from_slice(&more);
                    }
                    if let Ok(mut p) = s.screen.lock() {
                        p.process(&bytes);
                    }
                    behind.fetch_sub(bytes.len(), std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
        let mut out = std::fs::File::from_raw_fd(1);
        let mut buf = [0u8; 65536];
        loop {
            let n = libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
            if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if n <= 0 {
                break;
            }
            let bytes = &buf[..n as usize];
            let _ = out.write_all(bytes);
            while behind.load(std::sync::atomic::Ordering::Relaxed) > 16 << 20 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            behind.fetch_add(bytes.len(), std::sync::atomic::Ordering::Relaxed);
            let _ = keep.send(bytes.to_vec());
            shared.last_out.store(shared.started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        std::mem::forget(out);
        let _ = std::fs::remove_file(&socket);
        libc::tcsetattr(0, libc::TCSANOW, &outer);
        let mut status = 0;
        libc::waitpid(child, &mut status, 0);
        let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) };
        libc::_exit(code);
    }
}

fn write_all(fd: RawFd, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        // SAFETY: fd is the pty's master, open for the layer's whole life.
        let n = unsafe { libc::write(fd, bytes.as_ptr() as *const libc::c_void, bytes.len()) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

fn listen(path: &str) -> Option<std::os::unix::net::UnixListener> {
    let dir = std::path::Path::new(path).parent()?;
    std::fs::create_dir_all(dir).ok()?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::remove_file(path);
    std::os::unix::net::UnixListener::bind(path).ok()
}

/// What runs in the foreground of the shell's terminal: its process group's
/// leader, by name and the words it was started with, and where it is.
fn foreground(s: &Shared) -> (i32, String, String) {
    // SAFETY: asks the pty which group has it.
    let pg = unsafe { libc::tcgetpgrp(s.master) };
    let pid = if pg > 0 { pg } else { s.shell };
    let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let words: Vec<String> = cmd.split(|b| *b == 0).filter(|w| !w.is_empty()).map(|w| String::from_utf8_lossy(w).into_owned()).collect();
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).map(|p| p.display().to_string()).unwrap_or_default();
    (pid, words.join(" "), cwd)
}

/// Which coding agent that is, if one: by its name or the script it runs.
pub fn agent_of(command: &str) -> Option<&'static str> {
    const KNOWN: [(&str, &str); 10] = [
        ("claude", "claude"),
        ("codex", "codex"),
        ("gemini", "gemini"),
        ("opencode", "opencode"),
        ("copilot", "copilot"),
        ("qwen", "qwen"),
        ("aider", "aider"),
        ("goose", "goose"),
        ("crush", "crush"),
        ("pi", "pi"),
    ];
    let words: Vec<&str> = command.split_whitespace().take(3).collect();
    for w in words {
        let base = std::path::Path::new(w).file_name().and_then(|n| n.to_str()).unwrap_or(w);
        if let Some((_, a)) = KNOWN.iter().find(|(k, _)| base == *k || base.starts_with(&format!("{k}-")) || w.contains(&format!("/{k}/")) || w.contains(&format!("/{k}-")) || w.contains(&format!("@{k}"))) {
            return Some(a);
        }
    }
    None
}

/// What an agent's title says of it: Claude Code turns a half-moon while it
/// works (◐◓◑◒) and shows ✳ when it waits for the person; others spin
/// braille dots.
pub fn state_of(title: &str) -> &'static str {
    match title.chars().next() {
        Some('◐' | '◓' | '◑' | '◒') => "working",
        Some('⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏') => "working",
        Some('✳') => "idle",
        _ => "",
    }
}

/// The last `n` lines, history and screen, without the blank ones at the end.
fn last_lines(p: &mut vt100::Parser<Seen>, n: usize) -> String {
    let (rows, cols) = p.screen().size();
    let (rows, cols) = (rows.get() as usize, cols.get());
    let mut lines: Vec<String> = Vec::new();
    let back = p.screen().scrollback_capacity();
    // From the oldest still kept, a screen at a time.
    p.screen_mut().set_scrollback(back);
    let history = p.screen().scrollback();
    let mut offset = history;
    loop {
        p.screen_mut().set_scrollback(offset);
        let shown: Vec<String> = p.screen().rows(0, cols).collect();
        // A page `offset` back starts `offset` lines above the screen.
        let take = if offset >= rows { rows } else { offset };
        if offset == 0 {
            lines.extend(shown);
            break;
        }
        lines.extend(shown.into_iter().take(take));
        offset = offset.saturating_sub(take);
    }
    p.screen_mut().set_scrollback(0);
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let from = lines.len().saturating_sub(n);
    lines[from..].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

/// A key by its name, as the program asked for it (its cursor keys in
/// application mode or not).
fn key_bytes(name: &str, app_cursor: bool) -> Option<Vec<u8>> {
    let arrow = |c: u8| if app_cursor { vec![0x1b, b'O', c] } else { vec![0x1b, b'[', c] };
    Some(match name {
        "enter" | "return" => b"\r".to_vec(),
        "tab" => b"\t".to_vec(),
        "shift+tab" => b"\x1b[Z".to_vec(),
        "escape" | "esc" => b"\x1b".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "space" => b" ".to_vec(),
        "up" => arrow(b'A'),
        "down" => arrow(b'B'),
        "right" => arrow(b'C'),
        "left" => arrow(b'D'),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "page_up" | "pageup" => b"\x1b[5~".to_vec(),
        "page_down" | "pagedown" => b"\x1b[6~".to_vec(),
        _ if name.starts_with("ctrl+") && name.len() == 6 => {
            let c = name.as_bytes()[5].to_ascii_lowercase();
            if !c.is_ascii_lowercase() {
                return None;
            }
            vec![c - b'a' + 1]
        }
        _ if name.chars().count() == 1 => name.as_bytes().to_vec(),
        _ => return None,
    })
}

/// One question, one answer, and the connection closes. `info`, `read N`,
/// `paste HEX`, `keys NAME…`.
fn answer(s: &Shared, mut c: std::os::unix::net::UnixStream) {
    let mut line = String::new();
    {
        let mut r = std::io::BufReader::new(&c);
        use std::io::BufRead;
        if r.read_line(&mut line).is_err() {
            return;
        }
    }
    let mut words = line.trim_end().splitn(2, ' ');
    let verb = words.next().unwrap_or("");
    let rest = words.next().unwrap_or("");
    let reply = match verb {
        "info" => {
            let (title, bells, rows, cols, alt) = match s.screen.lock() {
                Ok(p) => {
                    let (r, c) = p.screen().size();
                    (p.callbacks().title.clone(), p.callbacks().bells, r.get(), c.get(), p.screen().alternate_screen())
                }
                Err(_) => return,
            };
            let (pid, cmd, cwd) = foreground(s);
            let quiet = s.started.elapsed().as_millis() as u64 - s.last_out.load(std::sync::atomic::Ordering::Relaxed);
            format!(
                "layer {}\nterminal {}\nshell {}\nforeground {pid}\ncommand {cmd}\ncwd {cwd}\nagent {}\ntitle {title}\nstate {}\nsize {cols}x{rows}\nfullscreen {}\nbells {bells}\nquiet {quiet}\n",
                // SAFETY: plain questions about this process.
                unsafe { libc::getpid() },
                unsafe { libc::getppid() },
                s.shell,
                agent_of(&cmd).unwrap_or(""),
                state_of(&title),
                alt as u8,
            )
        }
        "read" => {
            let n = rest.trim().parse::<usize>().unwrap_or(200).clamp(1, HISTORY);
            match s.screen.lock() {
                Ok(mut p) => last_lines(&mut p, n) + "\n",
                Err(_) => return,
            }
        }
        // Text as a paste: whole, bracketed if the program asked for that, so
        // an agent takes it as one thing and not as many lines typed.
        "paste" => match hex(rest.trim()) {
            Some(text) => {
                let bracketed = s.screen.lock().map(|p| p.screen().bracketed_paste()).unwrap_or(false);
                let _g = s.input.lock();
                if bracketed {
                    write_all(s.master, b"\x1b[200~");
                }
                write_all(s.master, &text);
                if bracketed {
                    write_all(s.master, b"\x1b[201~");
                }
                "ok\n".into()
            }
            None => "error: paste HEX\n".into(),
        },
        // Keys one by one, a little apart: an Escape and the key after it
        // are not read as Alt and that key.
        "keys" => {
            let app = s.screen.lock().map(|p| p.screen().application_cursor()).unwrap_or(false);
            let mut bad = None;
            let keys: Vec<Vec<u8>> = rest.split_whitespace().filter_map(|k| key_bytes(&k.to_lowercase(), app).or_else(|| { bad = Some(k.to_owned()); None })).collect();
            match bad {
                Some(k) => format!("error: no key called '{k}'\n"),
                None => {
                    for (i, k) in keys.iter().enumerate() {
                        if i > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        let _g = s.input.lock();
                        write_all(s.master, k);
                    }
                    "ok\n".into()
                }
            }
        }
        _ => "error: info, read N, paste HEX, keys NAME…\n".into(),
    };
    let _ = c.write_all(reply.as_bytes());
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Asks the layer at that socket one thing.
pub fn ask(socket: &str, question: &str) -> Result<String, String> {
    let mut c = std::os::unix::net::UnixStream::connect(socket).map_err(|e| e.to_string())?;
    let _ = c.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    c.write_all(format!("{question}\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    c.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The layers running now: their id and what each says of itself. A socket
/// left by one that is gone is taken away.
pub fn layers() -> Vec<(i32, std::collections::BTreeMap<String, String>)> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(socket_dir()) else { return out };
    for e in dir.flatten() {
        let path = e.path();
        let Some(id) = path.file_stem().and_then(|n| n.to_str()).and_then(|n| n.parse::<i32>().ok()) else { continue };
        match ask(&path.to_string_lossy(), "info") {
            Ok(text) => {
                let info = text.lines().filter_map(|l| l.split_once(' ').map(|(k, v)| (k.to_owned(), v.to_owned())).or_else(|| Some((l.to_owned(), String::new())))).collect();
                out.push((id, info));
            }
            // SAFETY: kill with 0 only asks whether it is there.
            Err(_) if unsafe { libc::kill(id, 0) } != 0 => {
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    out.sort_by_key(|(id, _)| *id);
    out
}

/// What an agent's screen shows when it asks the person something (a
/// permission, a choice): Claude Code's and Codex's questions.
fn asks_the_person(screen: &str) -> bool {
    const ASKS: [&str; 7] = ["Do you want to", "Would you like to", "❯ 1. Yes", "Allow command", "Approve", "(y/n)", "[y/N]"];
    let tail: Vec<&str> = screen.lines().rev().take(25).collect();
    tail.iter().any(|l| ASKS.iter().any(|a| l.contains(a)))
}

/// For the scene, every second: in which windows (`windows`, `slot pid`, as
/// pleamar-wm writes it) an agent runs in a terminal through a layer, and
/// how it is —`ag.N` 1 working, 2 ready, 3 waiting for the person; `agt.N`
/// «claude · ~/Proyectos/pleamar»—. Said only when it changes. In a thread of
/// its own: a terminal slow to answer never holds the compositor.
pub fn watch_agents(windows: String, to_render: std::sync::mpsc::Sender<pleamar::scene::ToRender>) {
    use pleamar::scene::{intern, ToRender};
    std::thread::spawn(move || {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut said: std::collections::HashMap<usize, (u8, String)> = std::collections::HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let text = std::fs::read_to_string(&windows).unwrap_or_default();
            let slots: Vec<(usize, u32)> = text.lines().filter_map(|l| {
                let mut w = l.split('\t').next()?.split_whitespace();
                Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?))
            }).collect();
            let all = if slots.is_empty() { Vec::new() } else { layers() };
            let mut now: std::collections::HashMap<usize, (u8, String)> = std::collections::HashMap::new();
            for (slot, pid) in &slots {
                let mine: Vec<&(i32, std::collections::BTreeMap<String, String>)> = all.iter().filter(|(_, i)| i.get("terminal").and_then(|t| t.parse::<u32>().ok()) == Some(*pid)).collect();
                // A terminal with several shells: the one with an agent, the busiest.
                let Some((id, info)) = mine.iter().filter(|(_, i)| i.get("agent").is_some_and(|a| !a.is_empty())).min_by_key(|(_, i)| i.get("quiet").and_then(|q| q.parse::<u64>().ok()).unwrap_or(u64::MAX)) else { continue };
                let agent = info.get("agent").cloned().unwrap_or_default();
                let cwd = info.get("cwd").cloned().unwrap_or_default();
                let cwd = if !home.is_empty() && cwd.starts_with(&home) { format!("~{}", &cwd[home.len()..]) } else { cwd };
                let working = info.get("state").map(String::as_str) == Some("working");
                let state = if working {
                    1
                } else if ask(&format!("{}/{id}.sock", socket_dir()), "read 25").is_ok_and(|s| asks_the_person(&s)) {
                    3
                } else {
                    2
                };
                now.insert(*slot, (state, format!("{agent} · {cwd}")));
            }
            let slots_known: std::collections::HashSet<usize> = said.keys().chain(now.keys()).copied().collect();
            for slot in slots_known {
                let new = now.get(&slot).cloned().unwrap_or((0, String::new()));
                if said.get(&slot) != Some(&new) {
                    let _ = to_render.send(ToRender::ExternalFact(intern(&format!("ag.{slot}")), new.0.to_string()));
                    let _ = to_render.send(ToRender::Text(intern(&format!("agt.{slot}")), new.1.clone()));
                    if new.0 == 0 {
                        said.remove(&slot);
                    } else {
                        said.insert(slot, new);
                    }
                }
            }
        }
    });
}

/// `pleamar-wm term list | read ID [N] | paste ID TEXT | keys ID NAME…`:
/// the layers, by hand (what `link` builds on).
pub fn cli(args: &[String]) -> i32 {
    let socket = |id: &str| format!("{}/{id}.sock", socket_dir());
    match args.first().map(String::as_str) {
        Some("list") | None => {
            for (id, info) in layers() {
                let g = |k: &str| info.get(k).cloned().unwrap_or_default();
                let agent = g("agent");
                println!(
                    "{id:>8}  terminal {}  {}{}  «{}»  {}",
                    g("terminal"),
                    if agent.is_empty() { g("command") } else { agent },
                    match g("state").as_str() {
                        "" => String::new(),
                        s => format!(" ({s})"),
                    },
                    g("title"),
                    g("cwd"),
                );
            }
            0
        }
        Some("info") if args.len() == 2 => print_reply(ask(&socket(&args[1]), "info")),
        Some("read") if args.len() >= 2 => print_reply(ask(&socket(&args[1]), &format!("read {}", args.get(2).map_or("200", String::as_str)))),
        Some("paste") if args.len() >= 3 => print_reply(ask(&socket(&args[1]), &format!("paste {}", to_hex(args[2..].join(" ").as_bytes())))),
        Some("keys") if args.len() >= 3 => print_reply(ask(&socket(&args[1]), &format!("keys {}", args[2..].join(" ")))),
        _ => {
            eprintln!("pleamar-wm term list | info ID | read ID [LINES] | paste ID TEXT | keys ID enter|tab|escape|up|ctrl+c…");
            2
        }
    }
}

fn print_reply(r: Result<String, String>) -> i32 {
    match r {
        Ok(text) if text.starts_with("error: ") => {
            eprint!("{text}");
            1
        }
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(e) => {
            eprintln!("no such terminal layer: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_title_says_whether_it_works() {
        assert_eq!(state_of("◑ Alternativa a quickshell"), "working");
        assert_eq!(state_of("✳ Modo Pokémon con chapas"), "idle");
        assert_eq!(state_of("⠹ codex"), "working");
        assert_eq!(state_of("abel@abel: ~"), "");
    }

    #[test]
    fn an_agent_is_known_by_what_it_runs() {
        assert_eq!(agent_of("claude --dangerously-skip-permissions"), Some("claude"));
        assert_eq!(agent_of("/usr/bin/codex"), Some("codex"));
        assert_eq!(agent_of("node /usr/lib/node_modules/@google/gemini-cli/dist/index.js"), Some("gemini"));
        assert_eq!(agent_of("vim notes.md"), None);
        assert_eq!(agent_of("pipewire"), None, "a name that starts like one is not it");
    }

    #[test]
    fn keys_are_sent_as_the_program_asked() {
        assert_eq!(key_bytes("up", false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes("up", true).unwrap(), b"\x1bOA");
        assert_eq!(key_bytes("ctrl+c", false).unwrap(), [3]);
        assert_eq!(key_bytes("enter", false).unwrap(), b"\r");
        assert!(key_bytes("hyper+q", false).is_none());
    }

    #[test]
    fn text_goes_through_as_hex() {
        assert_eq!(hex(&to_hex("echo «hola»\n".as_bytes())).unwrap(), "echo «hola»\n".as_bytes());
        assert!(hex("abc").is_none());
    }

    #[test]
    fn the_last_lines_come_from_the_history_and_the_screen() {
        let mut p = vt100::Parser::new_with_callbacks(nz(3), nz(20), HISTORY, Seen { title: String::new(), bells: 0 });
        p.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
        assert_eq!(last_lines(&mut p, 10), "one\ntwo\nthree\nfour\nfive");
        assert_eq!(last_lines(&mut p, 2), "four\nfive");
        p.process(b"\x1b]2;\xe2\x9c\xb3 here\x07");
        assert_eq!(p.callbacks().title, "✳ here");
    }
}
