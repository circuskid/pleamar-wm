//! Files between this desktop and the computer the remote page is on.
//!
//! Up: what is dropped on the page, or chosen with its button, comes one
//! file a request (`POST /upload`) into «Remote» in the downloads folder
//! here. Each name is only a name —no `..`, no `/`, nothing that is not
//! printed—, nothing already there is replaced («name (1)»), it is written
//! beside as `.part` and renamed once whole, and nothing comes executable.
//! Dropped on a window, the window is given them as if dragged here.
//!
//! Down: only what is offered from here —files copied in a file manager, or
//! `pleamar-wm remote send`—, never a path the page names. Each offer is a
//! random token, good for that signed-in session for a quarter of an hour.
//! One file goes as it is; several, or a folder, as a zip made on the way.

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// At most this much in one file sent here.
pub const FILE_MOST: u64 = 16 << 30;
/// What is always left free on the disk.
const ROOM_LEFT: u64 = 512 << 20;
/// How long an offer can be downloaded.
const OFFER_LIFE: Duration = Duration::from_secs(15 * 60);
/// How long the files of one sending are kept together (their folders' names).
const BATCH_LIFE: Duration = Duration::from_secs(60 * 60);
/// A zip without ZIP64: under 4 GB and 65 535 entries.
const ZIP_MOST: u64 = 0xffff_0000;
const ZIP_ENTRIES: usize = 65_000;

#[derive(Default)]
struct Files {
    batches: HashMap<String, Batch>,
    offers: HashMap<String, Offer>,
}

/// The files of one sending: the names their first folders got here, and
/// what was saved at the top.
struct Batch {
    session: String,
    began: Instant,
    tops: HashMap<String, PathBuf>,
    saved: Vec<PathBuf>,
    files: u32,
    bytes: u64,
}

struct Offer {
    paths: Vec<PathBuf>,
    session: String,
    until: Instant,
}

static FILES: LazyLock<Mutex<Files>> = LazyLock::new(Mutex::default);

/// Where what comes from the other computer lands: «Remote» («Remoto») in
/// the downloads folder (`XDG_DOWNLOAD_DIR`).
pub fn inbox() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let config = std::env::var("XDG_CONFIG_HOME").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
    let downloads = std::fs::read_to_string(format!("{config}/user-dirs.dirs"))
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                let v = l.trim().strip_prefix("XDG_DOWNLOAD_DIR=")?.trim_matches('"');
                Some(v.replace("$HOME", &home))
            })
        })
        .filter(|d| d.starts_with('/') && d != &home)
        .unwrap_or_else(|| format!("{home}/Downloads"));
    let spanish = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty())).is_some_and(|v| v.starts_with("es"));
    PathBuf::from(downloads).join(if spanish { "Remoto" } else { "Remote" })
}

/// One piece of a name as it may be here: without what is not printed,
/// slashes or a `..`, and not too long.
fn clean(piece: &str) -> Option<String> {
    let t: String = piece.chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').collect();
    let t = t.trim().trim_end_matches(['.', ' ']);
    if t.is_empty() || t == "." || t == ".." {
        return None;
    }
    let mut out = String::new();
    for c in t.chars() {
        if out.len() + c.len_utf8() > 200 {
            break;
        }
        out.push(c);
    }
    Some(out)
}

/// That name in that folder, or «name (1).ext» if it is taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if std::fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..10_000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| std::fs::symlink_metadata(p).is_err()).unwrap_or(first)
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(q: &str) -> HashMap<String, String> {
    q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_owned(), decode(v))).collect()
}

/// Free room on the disk that folder is on.
fn room(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: a zeroed statvfs filled by the call for a valid path.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

/// Renamed only if nothing has that name meanwhile.
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let (a, b) = (std::ffi::CString::new(from.as_os_str().as_bytes())?, std::ffi::CString::new(to.as_os_str().as_bytes())?);
    // SAFETY: two valid paths.
    let r = unsafe { libc::renameat2(libc::AT_FDCWD, a.as_ptr(), libc::AT_FDCWD, b.as_ptr(), libc::RENAME_NOREPLACE) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// A file sent from the page: `batch` (the sending it belongs to), `path`
/// (its name, with the folders it was in when a folder was dropped), and
/// `length` bytes of it in `body`. Answers where it was saved, or the
/// status and why not.
pub fn receive(body: &mut BufReader<TcpStream>, q: &str, length: u64, session: &str) -> Result<String, (&'static str, String)> {
    let q = query(q);
    let batch = q.get("batch").filter(|b| !b.is_empty() && b.len() <= 40 && b.chars().all(|c| c.is_ascii_alphanumeric())).ok_or(("400 Bad Request", "no sending".to_owned()))?;
    let pieces: Option<Vec<String>> = q.get("path").map(|p| p.split('/').filter(|s| !s.is_empty()).map(clean).collect()).unwrap_or(None);
    let pieces = pieces.filter(|p| !p.is_empty() && p.len() <= 32).ok_or(("400 Bad Request", "that name cannot be used".to_owned()))?;
    if length > FILE_MOST {
        return Err(("413 Content Too Large", format!("over {} GB", FILE_MOST >> 30)));
    }
    let inbox = inbox();
    std::fs::create_dir_all(&inbox).map_err(|e| ("500 Internal Server Error", format!("{}: {e}", inbox.display())))?;
    if room(&inbox).is_some_and(|r| r < length + ROOM_LEFT) {
        return Err(("507 Insufficient Storage", "no room left on the disk at home".to_owned()));
    }
    // Where it goes: its first folder named as the sending named it before.
    let (dir, name) = {
        let mut files = FILES.lock().unwrap();
        files.batches.retain(|_, b| b.began.elapsed() < BATCH_LIFE);
        let b = files.batches.entry(batch.clone()).or_insert_with(|| Batch { session: session.to_owned(), began: Instant::now(), tops: HashMap::new(), saved: Vec::new(), files: 0, bytes: 0 });
        if b.session != session {
            return Err(("403 Forbidden", "not yours".to_owned()));
        }
        if pieces.len() == 1 {
            (inbox.clone(), pieces[0].clone())
        } else {
            let top = match b.tops.get(&pieces[0]) {
                Some(t) => t.clone(),
                None => {
                    let t = free_name(&inbox, &pieces[0]);
                    std::fs::create_dir(&t).map_err(|e| ("500 Internal Server Error", format!("{}: {e}", t.display())))?;
                    b.tops.insert(pieces[0].clone(), t.clone());
                    b.saved.push(t.clone());
                    t
                }
            };
            let dir = pieces[1..pieces.len() - 1].iter().fold(top, |d, p| d.join(p));
            std::fs::create_dir_all(&dir).map_err(|e| ("500 Internal Server Error", format!("{}: {e}", dir.display())))?;
            (dir, pieces[pieces.len() - 1].clone())
        }
    };
    let part = free_name(&dir, &format!("{name}.part"));
    let written = (|| -> std::io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o644).open(&part)?;
        let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
        let copied = std::io::copy(&mut body.by_ref().take(length), &mut out)?;
        if copied != length {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the sending stopped halfway"));
        }
        out.flush()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(("400 Bad Request", e.to_string()));
    }
    // Its own name, unless something took it while it came.
    let mut last = Err(std::io::Error::other("no name"));
    for _ in 0..3 {
        let to = free_name(&dir, &name);
        last = rename_new(&part, &to).map(|_| to);
        if last.is_ok() {
            break;
        }
    }
    let saved = match last {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(("500 Internal Server Error", e.to_string()));
        }
    };
    let mut files = FILES.lock().unwrap();
    if let Some(b) = files.batches.get_mut(batch.as_str()) {
        if pieces.len() == 1 {
            b.saved.push(saved.clone());
        }
        b.files += 1;
        b.bytes += length;
    }
    Ok(saved.strip_prefix(&inbox).unwrap_or(&saved).display().to_string())
}

/// A sending is over: what it left at the top (to drop on a window), how
/// many files, how many bytes and how long it took.
pub fn finished(batch: &str, session: &str) -> Option<(Vec<PathBuf>, u32, u64, Duration)> {
    let mut files = FILES.lock().unwrap();
    let b = files.batches.remove(batch).filter(|b| b.session == session)?;
    Some((b.saved, b.files, b.bytes, b.began.elapsed()))
}

/// The files that clipboard text names (`text/uri-list`), if they are here.
pub fn paths_of_uris(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        .map(|l| l.strip_prefix("localhost").unwrap_or(l))
        .filter(|l| l.starts_with('/'))
        .map(|l| PathBuf::from(decode(&l.replace('+', "%2B"))))
        .filter(|p| std::fs::symlink_metadata(p).is_ok())
        .collect()
}

/// Paths as a `text/uri-list`.
pub fn uris_of_paths(paths: &[PathBuf]) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
    for p in paths {
        out.push_str("file://");
        for &b in p.as_os_str().as_bytes() {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out.push_str("\r\n");
    }
    out
}

/// What there is under those paths: (count of files, bytes), not following links.
fn measure(paths: &[PathBuf]) -> (usize, u64) {
    let (mut n, mut bytes) = (0, 0);
    walk(paths, &mut |_, _, m| {
        if m.is_file() {
            n += 1;
            bytes += m.len();
        }
    });
    (n, bytes)
}

/// Each entry under those paths, with its name in a zip (`dir/sub/file`).
fn walk(paths: &[PathBuf], each: &mut dyn FnMut(&Path, &str, &std::fs::Metadata)) {
    fn go(path: &Path, name: &str, each: &mut dyn FnMut(&Path, &str, &std::fs::Metadata), budget: &mut usize) {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(m) = std::fs::symlink_metadata(path) else { return };
        each(path, name, &m);
        if m.is_dir() {
            let Ok(list) = std::fs::read_dir(path) else { return };
            let mut entries: Vec<_> = list.filter_map(Result::ok).map(|e| e.path()).collect();
            entries.sort();
            for e in entries {
                let child = e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                go(&e, &format!("{name}/{child}"), each, budget);
            }
        }
    }
    let mut budget = ZIP_ENTRIES + 1;
    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "home".into());
        go(p, &name, each, &mut budget);
    }
}

/// Offers those paths to one signed-in session: its token, how many files,
/// how many bytes, and what it is called.
pub fn offer(paths: Vec<PathBuf>, session: &str, token: String) -> (String, usize, u64, String) {
    let (count, bytes) = measure(&paths);
    let name = match &paths[..] {
        [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        many => format!("{} items", many.len()),
    };
    let mut files = FILES.lock().unwrap();
    files.offers.retain(|_, o| o.until > Instant::now());
    files.offers.insert(token.clone(), Offer { paths, session: session.to_owned(), until: Instant::now() + OFFER_LIFE });
    (token, count, bytes, name)
}

fn disposition(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let mut enc = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{enc}")
}

const HEADERS: &str = "Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; sandbox\r\nConnection: close\r\n";

/// An offer downloaded: the file as it is, or a zip of them.
pub fn serve(stream: &mut TcpStream, token: &str, session: &str) -> Result<String, (&'static str, String)> {
    let paths = {
        let files = FILES.lock().unwrap();
        match files.offers.get(token) {
            Some(o) if o.session == session && o.until > Instant::now() => o.paths.clone(),
            Some(o) if o.until <= Instant::now() => return Err(("410 Gone", "that offer is over: copy it again at home".into())),
            _ => return Err(("404 Not Found", "nothing offered with that".into())),
        }
    };
    let err = |e: std::io::Error| ("500 Internal Server Error", e.to_string());
    let single = match &paths[..] {
        [one] => std::fs::metadata(one).ok().filter(|m| m.is_file()).map(|m| (one.clone(), m.len())),
        _ => None,
    };
    if let Some((path, len)) = single {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut file = std::fs::File::open(&path).map_err(err)?;
        let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\nContent-Disposition: {}\r\n{HEADERS}\r\n", disposition(&name));
        stream.write_all(head.as_bytes()).map_err(err)?;
        std::io::copy(&mut file, stream).map_err(err)?;
        return Ok(format!("{name} ({})", size(len)));
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    walk(&paths, &mut |p, name, m| {
        if m.is_file() || m.is_dir() {
            total += if m.is_file() { m.len() } else { 0 };
            entries.push((p.to_owned(), name.to_owned(), m.clone()));
        }
    });
    if total >= ZIP_MOST || entries.len() > ZIP_ENTRIES {
        return Err(("413 Content Too Large", "over 4 GB or 65 000 files in a zip: send them in parts".into()));
    }
    let name = match &paths[..] {
        [one] => format!("{}.zip", one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "home".into())),
        _ => "home.zip".to_owned(),
    };
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: {}\r\n{HEADERS}\r\n", disposition(&name));
    stream.write_all(head.as_bytes()).map_err(err)?;
    let mut out = std::io::BufWriter::with_capacity(1 << 20, stream);
    zip(&mut out, &entries).map_err(err)?;
    out.flush().map_err(err)?;
    Ok(format!("{name} ({} files, {})", entries.iter().filter(|e| e.2.is_file()).count(), size(total)))
}

pub fn size(b: u64) -> String {
    match b {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

/// A zip with nothing compressed, every size and checksum in its own
/// header: what every unzipper reads, Windows' own too. Each file is read
/// twice, first for its checksum.
fn zip(out: &mut impl Write, entries: &[(PathBuf, String, std::fs::Metadata)]) -> std::io::Result<()> {
    let mut central = Vec::new();
    let mut at = 0u64;
    let mut count = 0u16;
    for (path, name, m) in entries {
        let dir = m.is_dir();
        let name = if dir { format!("{name}/") } else { name.clone() };
        let (crc, len) = if dir {
            (0, 0)
        } else {
            let mut h = crc32fast::Hasher::new();
            let mut f = std::fs::File::open(path)?;
            let mut buf = vec![0; 1 << 16];
            let mut n = 0u64;
            loop {
                let k = f.read(&mut buf)?;
                if k == 0 {
                    break;
                }
                h.update(&buf[..k]);
                n += k as u64;
            }
            (h.finalize(), n)
        };
        let (time, date) = dos_time(m.modified().ok());
        let mode: u32 = if dir { 0o40755 } else { 0o100644 };
        let local = header(0x04034b50, None, time, date, crc, len as u32, &name);
        out.write_all(&local)?;
        if !dir {
            let mut f = std::fs::File::open(path)?;
            let copied = std::io::copy(&mut (&mut f).take(len), out)?;
            if copied != len {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "a file changed while it was sent"));
            }
        }
        central.extend(header(0x02014b50, Some(((mode << 16) | if dir { 0x10 } else { 0 }, at as u32)), time, date, crc, len as u32, &name));
        at += local.len() as u64 + len;
        count += 1;
    }
    out.write_all(&central)?;
    let mut end = Vec::new();
    end.extend(0x06054b50u32.to_le_bytes());
    end.extend([0u8; 4]);
    end.extend(count.to_le_bytes());
    end.extend(count.to_le_bytes());
    end.extend((central.len() as u32).to_le_bytes());
    end.extend((at as u32).to_le_bytes());
    end.extend([0u8; 2]);
    out.write_all(&end)
}

/// A local header, or with `central` (external attributes, offset) the
/// central directory's: stored, names in UTF-8.
fn header(signature: u32, central: Option<(u32, u32)>, time: u16, date: u16, crc: u32, len: u32, name: &str) -> Vec<u8> {
    let mut h = Vec::with_capacity(46 + name.len());
    h.extend(signature.to_le_bytes());
    if central.is_some() {
        h.extend(0x031eu16.to_le_bytes()); // made by: Unix, 3.0
    }
    h.extend(20u16.to_le_bytes());
    h.extend(0x0800u16.to_le_bytes());
    h.extend(0u16.to_le_bytes());
    h.extend(time.to_le_bytes());
    h.extend(date.to_le_bytes());
    h.extend(crc.to_le_bytes());
    h.extend(len.to_le_bytes());
    h.extend(len.to_le_bytes());
    h.extend((name.len() as u16).to_le_bytes());
    h.extend(0u16.to_le_bytes());
    if let Some((attrs, offset)) = central {
        h.extend([0u8; 6]); // comment, disk, internal attributes
        h.extend(attrs.to_le_bytes());
        h.extend(offset.to_le_bytes());
    }
    h.extend(name.as_bytes());
    h
}

/// MS-DOS time and date, as zips keep them (in UTC).
fn dos_time(t: Option<SystemTime>) -> (u16, u16) {
    let secs = t.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()).max(315_532_800);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days to a date (Howard Hinnant's).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    let time = (((rem / 3600) << 11) | ((rem % 3600 / 60) << 5) | ((rem % 60) / 2)) as u16;
    let date = (((year - 1980).clamp(0, 127) << 9) | (month << 5) | day) as u16;
    (time, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_stay_names() {
        assert_eq!(clean("../etc"), Some("..etc".into()));
        assert_eq!(clean(".."), None);
        assert_eq!(clean("a/b\\c"), Some("abc".into()));
        assert_eq!(clean("x\u{0}y\n"), Some("xy".into()));
        assert_eq!(clean("  report.pdf. "), Some("report.pdf".into()));
        assert_eq!(clean("..."), None);
        assert!(clean(&"é".repeat(300)).unwrap().len() <= 200);
    }

    #[test]
    fn uris_go_and_come_back() {
        let p = vec![PathBuf::from("/tmp")];
        assert_eq!(paths_of_uris(&uris_of_paths(&p)), p);
        assert_eq!(decode("a%20b%2Fc"), "a b/c");
        assert_eq!(dos_time(Some(UNIX_EPOCH + Duration::from_secs(1_791_936_000))), (0, (46 << 9) | (10 << 5) | 14));
    }
}
