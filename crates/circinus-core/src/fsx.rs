//! Folders that are really somewhere else. Mod managers and dev tools (Modmixer, Circinus Dev
//! Tools, a hand-made `mklink`) often put a link in the Mods folder, a symlink or a junction on
//! Windows, that points at a working copy kept elsewhere. RimWorld reads straight through such
//! links, so Circinus has to see the same mods. Opening a link and letting the OS follow it does
//! not work everywhere (Windows refuses in some setups), so the link target is read and used
//! directly, for the entry in the Mods folder and for links inside a mod.

use std::fs::Metadata;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// The folder or file a path really names: follows a chain of links (at most 8 hops) by reading
/// each link's target. The path comes back unchanged when it is not a link or cannot be read.
pub fn real_root(path: &Path) -> PathBuf {
    let mut cur = path.to_path_buf();
    for _ in 0..8 {
        let Ok(md) = std::fs::symlink_metadata(&cur) else { break };
        if !md.file_type().is_symlink() {
            break;
        }
        let Ok(target) = std::fs::read_link(&cur) else { break };
        cur = tidy(if target.is_absolute() { target } else { cur.parent().map(|p| p.join(&target)).unwrap_or(target) });
    }
    cur
}

/// Junction targets sometimes carry a trailing separator; drop it unless that would leave a
/// bare drive or root.
fn tidy(p: PathBuf) -> PathBuf {
    let Some(s) = p.to_str() else { return p };
    let t = s.trim_end_matches(['\\', '/']);
    if t.len() < s.len() && t.contains(['\\', '/']) {
        PathBuf::from(t)
    } else {
        p
    }
}

/// The real folder behind a Mods folder entry, or why there is none: a plain folder gives
/// itself back, a link gives its target when that is a folder.
pub fn folder_at(path: &Path) -> std::result::Result<PathBuf, String> {
    let real = real_root(path);
    match std::fs::metadata(&real) {
        Ok(md) if md.is_dir() => return Ok(real),
        Ok(_) if real != path => return Err(format!("links to {}, which is a file, not a folder", real.display())),
        Ok(_) => return Err("is a file".into()),
        Err(e) if real != path => return Err(format!("links to {}, which cannot be read: {}", real.display(), plain(&e))),
        Err(e) => {
            // A link of a kind this platform cannot read, or one the OS follows only when the
            // folder is listed: try that before giving up.
            if std::fs::read_dir(path).is_ok() {
                return Ok(real);
            }
            Err(format!("cannot be opened: {}", plain(&e)))
        }
    }
}

fn plain(e: &std::io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error").next().unwrap_or(&s).trim_end_matches('.').to_string()
}

/// A file or folder found under a mod folder.
#[derive(Debug)]
pub struct Found {
    /// Where it really is (through any links).
    pub path: PathBuf,
    /// Its path relative to the mod folder, the way RimWorld sees it.
    pub rel: PathBuf,
    pub meta: Metadata,
}

/// Every file and folder under `root`, read through links the way RimWorld reads them: the
/// entry in the Mods folder may be a link, and so may folders inside the mod. `skip(name)`
/// prunes folders below the root by name. A link back to a folder already being walked is
/// left alone so a loop cannot run away.
pub fn walk(root: &Path, skip: &dyn Fn(&str) -> bool) -> Vec<Found> {
    let mut out = Vec::new();
    let Ok(real) = folder_at(root) else { return out };
    let mut stack: Vec<PathBuf> = Vec::new();
    walk_into(&real, Path::new(""), skip, &mut stack, &mut out);
    out
}

fn walk_into(dir: &Path, prefix: &Path, skip: &dyn Fn(&str) -> bool, stack: &mut Vec<PathBuf>, out: &mut Vec<Found>) {
    if stack.len() > 4 || stack.iter().any(|s| s.starts_with(dir)) {
        return;
    }
    stack.push(dir.to_path_buf());
    let walker = WalkDir::new(dir).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !skip(&e.file_name().to_string_lossy()));
    for entry in walker.filter_map(|e| e.ok()) {
        if entry.depth() == 0 {
            continue;
        }
        let rel = prefix.join(entry.path().strip_prefix(dir).unwrap_or(entry.path()));
        if entry.file_type().is_symlink() {
            let target = real_root(entry.path());
            match std::fs::metadata(&target) {
                Ok(meta) if meta.is_dir() => walk_into(&target, &rel, skip, stack, out),
                Ok(meta) => out.push(Found { path: target, rel, meta }),
                Err(_) => {}
            }
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        out.push(Found { path: entry.path().to_path_buf(), rel, meta });
    }
    stack.pop();
}

/// Forward-slash form of a relative path, for inventories and manifests.
pub fn rel_str(rel: &Path) -> String {
    rel.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[cfg(unix)]
    fn link_dir(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }
    #[cfg(windows)]
    fn link_dir(target: &Path, link: &Path) {
        // A junction needs no privilege on Windows; a symlink would. mklink is a cmd builtin, so
        // it has to go through cmd, and cmd reads a leading / as a switch: a path carrying forward
        // slashes, which Rust's own file APIs take happily, arrives here as "Invalid switch".
        let native = |p: &Path| p.to_string_lossy().replace('/', "\\");
        let out = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(native(link)).arg(native(target)).output().unwrap();
        assert!(out.status.success(), "mklink {} {}: {}{}", native(link), native(target), String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }

    /// A path built for comparing against one the code produced. On Windows "a/b" and "a\\b" are
    /// the same path but not the same string, and what comes back from the filesystem carries
    /// the platform's own separator; anything asserted against it has to be built the same way.
    fn at(base: &std::path::Path, rel: &str) -> std::path::PathBuf {
        rel.split('/').fold(base.to_path_buf(), |p, part| p.join(part))
    }

    #[test]
    fn walks_through_links_and_stops_loops() {
        let tmp = tempfile::tempdir().unwrap();
        let work = at(tmp.path(), "work/mymod");
        write(&work.join("About/About.xml"), "<ModMetaData/>");
        write(&work.join("Defs/A.xml"), "<Defs/>");
        let shared = at(tmp.path(), "shared/Textures/Things/Wall.png");
        write(&shared, "png");
        link_dir(&tmp.path().join("shared/Textures"), &work.join("Textures"));
        // a link back up to the mod itself: must not recurse forever
        link_dir(&work, &work.join("Defs/loop"));
        let link = at(tmp.path(), "Mods/mymod");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        link_dir(&work, &link);

        assert_eq!(real_root(&link), work);
        assert_eq!(folder_at(&link).unwrap(), work);
        let found = walk(&link, &|n| n == ".git");
        let rels: Vec<String> = found.iter().filter(|f| f.meta.is_file()).map(|f| rel_str(&f.rel)).collect();
        assert!(rels.contains(&"About/About.xml".to_string()), "{rels:?}");
        assert!(rels.contains(&"Defs/A.xml".to_string()), "{rels:?}");
        assert!(rels.contains(&"Textures/Things/Wall.png".to_string()), "read through the link inside the mod: {rels:?}");
        assert!(!rels.iter().any(|r| r.starts_with("Defs/loop/")), "the loop is cut: {rels:?}");
        let wall = found.iter().find(|f| rel_str(&f.rel) == "Textures/Things/Wall.png").unwrap();
        assert_eq!(wall.path, shared, "the real file, not the path through the link");
    }

    #[test]
    fn dangling_links_are_explained() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("gone");
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("nowhere"), &link).unwrap();
        #[cfg(windows)]
        {
            fs::create_dir_all(tmp.path().join("nowhere")).unwrap();
            link_dir(&tmp.path().join("nowhere"), &link);
            fs::remove_dir(tmp.path().join("nowhere")).unwrap();
        }
        let why = folder_at(&link).unwrap_err();
        assert!(why.contains("nowhere"), "{why}");
        assert!(walk(&link, &|_| false).is_empty());
        let file = tmp.path().join("file.txt");
        write(&file, "x");
        assert_eq!(folder_at(&file).unwrap_err(), "is a file");
    }
}
