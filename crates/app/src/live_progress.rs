//! One in-flight BookOrbit request per open edition. The UI owns the latest
//! unsent move, so slow acknowledgements cannot discard subsequent page turns.
use botui_bookorbit::{
    Session,
    progress::{Move, Progress, Target},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const PAUSE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_secs(15);

type SyncJob = JoinHandle<botui_bookorbit::Result<(Progress, Option<Move>)>>;

pub struct Live {
    session: Arc<Session>,
    path: PathBuf,
    pub base: Progress,
    pub pending: Option<Move>,
    pub reopen: bool,
    send_at: Instant,
    poll_at: Instant,
    job: Option<SyncJob>,
}

impl Live {
    pub fn new(session: Arc<Session>, path: &Path, base: Progress) -> Self {
        Self {
            session,
            path: path.into(),
            base,
            pending: None,
            reopen: false,
            send_at: Instant::now(),
            poll_at: Instant::now() + POLL,
            job: None,
        }
    }

    pub fn moved(&mut self, change: Option<Move>) {
        self.pending = change;
        self.send_at = Instant::now() + PAUSE;
    }
    pub fn refresh(&mut self) {
        self.poll_at = Instant::now();
    }
    pub fn timeout(&self) -> Duration {
        if self.job.is_some() {
            return Duration::from_millis(150);
        }
        let next = if self.pending.is_some() {
            self.send_at
        } else {
            self.poll_at
        };
        next.saturating_duration_since(Instant::now())
    }
    pub fn tick(&mut self, term: &mut crate::AppTerminal) -> std::io::Result<bool> {
        if self.job.as_ref().is_some_and(|j| j.is_finished()) {
            if let Some(remote) = self.collect()? {
                self.resolve(term, remote)?;
                self.pending = None;
                return Ok(true);
            }
        } else if self.job.is_none() && self.timeout().is_zero() {
            self.poll_at = Instant::now() + POLL;
            let session = self.session.clone();
            let base = self.base.clone();
            let change = self.pending.clone();
            self.job = Some(thread::spawn(move || {
                let saved = if let Some(m) = change {
                    session
                        .with_token(|a, t| a.save_move(t, &base, &m))?
                        .then_some(m)
                } else {
                    None
                };
                let current = session.with_token(|a, t| a.progress(t, base.file_id))?;
                Ok((current, saved))
            }));
        }
        Ok(false)
    }
    fn resolve(
        &mut self,
        term: &mut crate::AppTerminal,
        mut remote: Progress,
    ) -> std::io::Result<()> {
        let file = remote.file_id;
        let latest = || {
            self.session
                .with_token(|api, token| api.progress(token, file))
        };
        let label = |p: &Progress| format!("Go to BookOrbit position · {:.0}%", p.percentage);
        let retry = "BookOrbit unavailable · retry";
        let mut action = label(&remote);
        loop {
            let choice = crate::prompt::choose_live(
                term,
                "Reading position changed on another device",
                vec![action, "Stay here".into()],
                POLL,
                |labels| {
                    labels[0] = match latest() {
                        Ok(p) => {
                            remote = p;
                            label(&remote)
                        }
                        Err(_) => retry.into(),
                    };
                },
            )?;
            if choice != Some(0) {
                break;
            }
            // The web reader can advance again between painting the label and the click.
            match latest() {
                Ok(p) => {
                    remote = p;
                    self.reopen = true;
                    break;
                }
                Err(_) => action = retry.into(),
            }
        }
        self.accept(remote)
    }
    fn accept(&mut self, p: Progress) -> std::io::Result<()> {
        crate::offline::advance(&self.path, &self.base, p.clone())?;
        self.base = p;
        Ok(())
    }
    // On close, leave any conflicting final move to the shelf's guarded save.
    pub fn collect(&mut self) -> std::io::Result<Option<Progress>> {
        let Some(job) = self.job.take() else {
            return Ok(None);
        };
        let Ok(Ok((p, saved))) = job.join() else {
            self.send_at = Instant::now() + POLL;
            return Ok(None);
        };
        if let Some(m) = saved
            && matches_position(&p, &m)
        {
            if self.pending.as_ref() == Some(&m) {
                self.pending = None;
            }
            self.accept(p)?;
        } else if self
            .pending
            .as_ref()
            .is_some_and(|m| matches_position(&p, m))
        {
            self.pending = None;
            self.accept(p)?;
        } else if p != self.base {
            return Ok(Some(p));
        } else if self.pending.is_some() && self.send_at <= Instant::now() {
            self.send_at = Instant::now() + PAUSE;
        }
        Ok(None)
    }
}
fn matches_position(p: &Progress, m: &Move) -> bool {
    (p.percentage - m.percentage).abs() < 0.51
        && match &m.target {
            Target::Text(c) => p.text.cfi.as_ref() == Some(c),
            Target::Page(n) => p.text.page == Some(*n),
        }
}
