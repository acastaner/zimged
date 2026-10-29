//! Sibling navigation (§3.1–3.3): the scan, the position in the list, moving to the next or
//! previous Sibling, and the prefetch window. The app owns one [`Navigation`].

use std::path::{Path, PathBuf};

use zimged_codec::Size;
use zimged_siblings::{
    Decoded, Direction, PendingScan, Prefetcher, SiblingList, prefetch, prefetch_window,
};

/// The transient messages at the ends of the list (§3.1).
pub const FIRST_IMAGE: &str = "First image in folder";
pub const LAST_IMAGE: &str = "Last image in folder";

enum Scan {
    NotStarted,
    Running(PendingScan),
    /// `None` when the directory couldn't be read.
    Done(Option<SiblingList>),
}

/// What a move to the next or previous Sibling did.
pub enum Step {
    /// The Sibling was cached: show it now.
    Show(PathBuf, Decoded),
    /// The Sibling is being decoded; the previous Image stays on screen until [`Navigation::poll`]
    /// hands it over.
    Pending,
    /// Already at the end of the list: show this message.
    End(&'static str),
    /// The Siblings aren't known yet (the scan hasn't finished).
    NotReady,
}

pub struct Navigation {
    scan: Scan,
    prefetch: Option<Prefetcher>,
    cache_cap: usize,
    direction: Direction,
    /// The Sibling being moved to while its decode is pending.
    target: Option<PathBuf>,
    /// Files trashed from the folder being scanned: left out of every scan of it, which may
    /// predate the trashing.
    trashed: Vec<PathBuf>,
}

impl Navigation {
    pub fn new(cache_cap: usize) -> Self {
        Self {
            scan: Scan::NotStarted,
            prefetch: None,
            cache_cap,
            direction: Direction::Forward,
            target: None,
            trashed: Vec::new(),
        }
    }

    /// Scan the directory of `shown`, the Image now on screen, and start the decode pool if it isn't
    /// running yet. Only ever called after the first image is painted (§1.3), or by
    /// [`Self::ensure_started`].
    pub fn start(&mut self, ctx: &egui::Context, shown: &Path) {
        let dir = folder_of(shown);
        let waker = ctx.clone();
        self.trashed.retain(|path| folder_of(path) == dir);
        self.scan = Scan::Running(PendingScan::spawn(
            dir,
            zimged_siblings::order::platform_order,
            move || waker.request_repaint(),
        ));
        if self.prefetch.is_none() {
            let waker = ctx.clone();
            self.prefetch = Some(Prefetcher::new(
                prefetch::pool_size(),
                self.cache_cap,
                move || waker.request_repaint(),
            ));
        }
        self.target = None;
        self.direction = Direction::Forward;
    }

    /// Forget a pending move (another file is being opened).
    pub fn cancel(&mut self) {
        self.target = None;
    }

    /// Collect background results: a finished scan starts the prefetch around `shown`; a pending
    /// move's decode is returned once it has arrived.
    pub fn poll(&mut self, shown: Option<&Path>, display_box: Size) -> Option<(PathBuf, Decoded)> {
        if let Scan::Running(pending) = &self.scan
            && let Some(result) = pending.take()
        {
            crate::bench::mark("siblings_scanned");
            let mut list = result.ok();
            if let Some(list) = &mut list {
                for path in &self.trashed {
                    list.remove(path);
                }
            }
            self.scan = Scan::Done(list);
            if let Some(shown) = shown {
                self.keep_window(shown, display_box);
            }
        }
        let target = self.target.as_ref()?;
        let decoded = self.prefetch.as_ref()?.get(target)?;
        let target = self.target.take()?;
        Some((target, decoded))
    }

    /// Move one Sibling forward or back from the Image on screen (or from the pending target).
    pub fn go(&mut self, shown: &Path, direction: Direction, display_box: Size) -> Step {
        let (Scan::Done(Some(list)), Some(prefetch)) = (&self.scan, &self.prefetch) else {
            return Step::NotReady;
        };
        let from = self.target.as_deref().unwrap_or(shown);
        let len = list.len();
        let next = match (list.locate(from), direction) {
            (Ok(i), Direction::Forward) => Some(i + 1).filter(|&i| i < len),
            (Ok(i), Direction::Backward) => i.checked_sub(1),
            (Err(i), Direction::Forward) => Some(i).filter(|&i| i < len),
            (Err(i), Direction::Backward) => i.checked_sub(1),
        };
        let Some(index) = next else {
            return Step::End(match direction {
                Direction::Forward => LAST_IMAGE,
                Direction::Backward => FIRST_IMAGE,
            });
        };
        self.direction = direction;
        let path = list.get(index).expect("index in range").path.clone();
        let window = window_paths(list, index, direction);
        match prefetch.show(&path, &window, display_box) {
            Some(decoded) => {
                self.target = None;
                Step::Show(path, decoded)
            }
            None => {
                self.target = Some(path);
                Step::Pending
            }
        }
    }

    /// Prefetch around `shown`, which is on screen already.
    fn keep_window(&self, shown: &Path, display_box: Size) {
        let (Scan::Done(Some(list)), Some(prefetch)) = (&self.scan, &self.prefetch) else {
            return;
        };
        let Ok(index) = list.locate(shown) else {
            return;
        };
        prefetch.keep(
            shown,
            &window_paths(list, index, self.direction),
            display_box,
        );
    }

    /// The status-bar position (§2): `n / N`, `? / ?` until the scan has finished, `– / N` for an
    /// Image that isn't one of the Siblings (a hidden file, say).
    pub fn position(&self, shown: &Path) -> String {
        match &self.scan {
            Scan::Done(Some(list)) => match list.locate(shown) {
                Ok(i) => format!("{} / {}", i + 1, list.len()),
                Err(_) => format!("– / {}", list.len()),
            },
            _ => "? / ?".to_owned(),
        }
    }

    /// Take a trashed file out of the Siblings, now and from any scan landing later. A move from
    /// it afterwards goes to the Sibling that followed it (or preceded it, backward).
    pub fn forget(&mut self, path: &Path) {
        self.target = None;
        if let Scan::Done(Some(list)) = &mut self.scan {
            list.remove(path);
        }
        self.trashed.push(path.to_owned());
    }

    /// Scan the folder of `path` unless a scan has been started already (an Image that failed to
    /// open at launch is never painted, so nothing else starts one).
    pub fn ensure_started(&mut self, ctx: &egui::Context, path: &Path) {
        if matches!(self.scan, Scan::NotStarted) {
            self.start(ctx, path);
        }
    }

    /// Has the scan finished, so that moves work?
    pub fn is_scanned(&self) -> bool {
        matches!(self.scan, Scan::Done(_))
    }

    /// Is a scan, a pending move or a prefetch decode still running?
    pub fn is_busy(&self) -> bool {
        matches!(self.scan, Scan::Running(_))
            || self.target.is_some()
            || self.prefetch.as_ref().is_some_and(|p| !p.is_idle())
    }

    /// The files in the display-size cache and their total size in bytes.
    pub fn cached(&self) -> (Vec<PathBuf>, usize) {
        self.prefetch
            .as_ref()
            .map_or_else(Default::default, Prefetcher::cached)
    }
}

fn folder_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_owned()
}

fn window_paths(list: &SiblingList, index: usize, direction: Direction) -> Vec<PathBuf> {
    prefetch_window(list.len(), index, direction)
        .into_iter()
        .filter_map(|i| list.get(i).map(|s| s.path.clone()))
        .collect()
}
